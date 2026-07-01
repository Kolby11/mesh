use crate::template::*;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

use super::ParseError;
use crate::ComponentImportTarget;

/// Convert `{#for}`, `{#if}`, `{:else if}`, `{:else}`, `{/for}`, `{/if}` directives
/// into custom XML tags so quick_xml can build a proper element tree.
///
/// `{#for item in list}` → `<mesh-for item="item" iterable="list">`
/// `{/for}`              → `</mesh-for>`
/// `{#if cond}`          → `<mesh-if><mesh-ifthen condition="ESCAPED">`
/// `{:else if cond}`     → `</mesh-ifthen><mesh-ifthen condition="ESCAPED">`
/// `{:else}`             → `</mesh-ifthen><mesh-else>`
/// `{/if}`               → close current branch + `</mesh-if>`
fn preprocess_control_flow(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 128);
    let mut remaining = source;
    // Stack entries: "for" | "if-outer" | "ifthen" | "else"
    let mut cf_stack: Vec<&'static str> = Vec::new();

    while !remaining.is_empty() {
        let Some(brace_pos) = remaining.find('{') else {
            out.push_str(remaining);
            break;
        };

        out.push_str(&remaining[..brace_pos]);
        remaining = &remaining[brace_pos..];

        // {#for item in iterable}
        if let Some(rest) = remaining.strip_prefix("{#for ") {
            if let Some(end) = find_cf_end(rest) {
                let inner = rest[..end].trim();
                if let Some(sep) = inner.find(" in ") {
                    let item = inner[..sep].trim();
                    let iterable = inner[sep + 4..].trim();
                    out.push_str(&format!(
                        "<mesh-for item=\"{}\" iterable=\"{}\">",
                        xml_attr_escape(item),
                        xml_attr_escape(iterable)
                    ));
                    cf_stack.push("for");
                    remaining = &rest[end + 1..];
                    continue;
                }
            }
        }
        // {/for}
        else if remaining.starts_with("{/for}") {
            out.push_str("</mesh-for>");
            if cf_stack.last() == Some(&"for") {
                cf_stack.pop();
            }
            remaining = &remaining[6..];
            continue;
        }
        // {#if condition}
        else if let Some(rest) = remaining.strip_prefix("{#if ") {
            if let Some(end) = find_cf_end(rest) {
                let cond = rest[..end].trim();
                out.push_str(&format!(
                    "<mesh-if><mesh-ifthen condition=\"{}\">",
                    xml_attr_escape(cond)
                ));
                cf_stack.push("if-outer");
                cf_stack.push("ifthen");
                remaining = &rest[end + 1..];
                continue;
            }
        }
        // {:else if condition}  — must be checked before {:else}
        else if let Some(rest) = remaining.strip_prefix("{:else if ") {
            if let Some(end) = find_cf_end(rest) {
                let cond = rest[..end].trim();
                match cf_stack.last() {
                    Some(&"ifthen") => {
                        out.push_str("</mesh-ifthen>");
                        cf_stack.pop();
                    }
                    Some(&"else") => {
                        out.push_str("</mesh-else>");
                        cf_stack.pop();
                    }
                    _ => {}
                }
                out.push_str(&format!(
                    "<mesh-ifthen condition=\"{}\">",
                    xml_attr_escape(cond)
                ));
                cf_stack.push("ifthen");
                remaining = &rest[end + 1..];
                continue;
            }
        }
        // {:else}
        else if remaining.starts_with("{:else}") {
            match cf_stack.last() {
                Some(&"ifthen") => {
                    out.push_str("</mesh-ifthen>");
                    cf_stack.pop();
                }
                Some(&"else") => {
                    out.push_str("</mesh-else>");
                    cf_stack.pop();
                }
                _ => {}
            }
            out.push_str("<mesh-else>");
            cf_stack.push("else");
            remaining = &remaining[7..];
            continue;
        }
        // {/if}
        else if remaining.starts_with("{/if}") {
            match cf_stack.last() {
                Some(&"ifthen") => {
                    out.push_str("</mesh-ifthen>");
                    cf_stack.pop();
                }
                Some(&"else") => {
                    out.push_str("</mesh-else>");
                    cf_stack.pop();
                }
                _ => {}
            }
            if cf_stack.last() == Some(&"if-outer") {
                out.push_str("</mesh-if>");
                cf_stack.pop();
            }
            remaining = &remaining[5..];
            continue;
        }

        // Not a control-flow token — keep `{` and advance.
        out.push('{');
        remaining = &remaining[1..];
    }

    out
}

/// Find the index of the `}` that closes the outer `{`, depth-aware.
/// `s` is the text AFTER the opening `{`.
fn find_cf_end(s: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Escape special XML attribute characters so conditions survive the round-trip
/// through quick_xml (it will unescape them back when reading the attribute).
fn xml_attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Convert unquoted brace attribute values to quoted form so quick_xml can parse them.
///
/// `onclick={handler}` → `onclick="{handler}"`
/// `value={expr}` follows the same preprocessing path before XML parsing.
fn preprocess_template(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 32);
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_tag = false;
    let mut in_quoted = false;
    let mut quote_char = b'"';

    while i < len {
        let b = bytes[i];

        if !in_tag {
            if b == b'<' {
                in_tag = true;
            }
            out.push(b as char);
            i += 1;
        } else if in_quoted {
            if b == quote_char {
                in_quoted = false;
            }
            out.push(b as char);
            i += 1;
        } else if b == b'"' || b == b'\'' {
            in_quoted = true;
            quote_char = b;
            out.push(b as char);
            i += 1;
        } else if b == b'>' {
            in_tag = false;
            out.push(b as char);
            i += 1;
        } else if b == b'=' && i + 1 < len && bytes[i + 1] == b'{' {
            // Unquoted brace value: wrap it.
            out.push('=');
            out.push('"');
            i += 1; // skip '=', now pointing at '{'
            let mut depth: i32 = 0;
            while i < len {
                let c = bytes[i] as char;
                out.push(c);
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            out.push('"');
        } else {
            out.push(b as char);
            i += 1;
        }
    }

    out
}

pub(super) fn parse_markup(
    source: &str,
    imported_components: &HashMap<String, ComponentImportTarget>,
) -> Result<TemplateBlock, ParseError> {
    let cf_processed = preprocess_control_flow(source.trim());
    let preprocessed = preprocess_template(&cf_processed);
    let wrapped = format!("<mesh-root>{}</mesh-root>", preprocessed);
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);

    let mut stack: Vec<OpenNode> = Vec::new();
    let mut root = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    continue;
                }
                let attrs = parse_xml_attributes(&reader, &event)?;
                stack.push(OpenNode {
                    tag,
                    attributes: attrs,
                    children: Vec::new(),
                });
            }
            Ok(Event::Empty(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    continue;
                }
                let attrs = parse_xml_attributes(&reader, &event)?;
                let node = build_template_node(tag, attrs, Vec::new(), imported_components)?;
                push_template_node(&mut stack, &mut root, node);
            }
            Ok(Event::Text(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                for node in parse_inline_nodes(&text) {
                    push_template_node(&mut stack, &mut root, node);
                }
            }
            Ok(Event::CData(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                for node in parse_inline_nodes(&text) {
                    push_template_node(&mut stack, &mut root, node);
                }
            }
            Ok(Event::End(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    break;
                }

                let open = stack.pop().ok_or_else(|| ParseError::UnexpectedClose {
                    tag: tag.clone(),
                    line: 0,
                })?;

                if open.tag != tag {
                    return Err(ParseError::UnexpectedClose { tag, line: 0 });
                }

                let node = build_template_node(
                    open.tag,
                    open.attributes,
                    open.children,
                    imported_components,
                )?;
                push_template_node(&mut stack, &mut root, node);
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_))
            | Ok(Event::Decl(_))
            | Ok(Event::PI(_))
            | Ok(Event::DocType(_))
            | Ok(Event::GeneralRef(_)) => {}
            Err(err) => {
                return Err(ParseError::InvalidTemplate {
                    message: err.to_string(),
                });
            }
        }
    }

    if let Some(open) = stack.pop() {
        return Err(ParseError::UnclosedBlock {
            tag: open.tag,
            line: 0,
        });
    }

    Ok(TemplateBlock { root })
}

fn parse_xml_attributes(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<Vec<Attribute>, ParseError> {
    let mut attrs = Vec::new();

    for attr in event.attributes().with_checks(false) {
        let attr = attr.map_err(|err| ParseError::InvalidTemplate {
            message: err.to_string(),
        })?;
        let name = decode_name(attr.key.as_ref());
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|err| ParseError::InvalidTemplate {
                message: err.to_string(),
            })?
            .into_owned();

        let (attr_name, attr_value) = if name == "bind:this" {
            let binding = extract_brace_expr(&value).unwrap_or(value);
            (name, AttributeValue::InstanceBinding(binding))
        } else if let Some(var) = name.strip_prefix("bind:") {
            // bind:value="variable" — two-way binding.
            (var.to_string(), AttributeValue::TwoWayBinding(value))
        } else if is_event_attr(&name) {
            // onclick={handler}, onclick="handler", or onclick="{handler}" — strip braces if present.
            let handler = extract_brace_expr(&value).unwrap_or(value);
            if let Some((fn_name, fn_args)) = parse_handler_call(&handler) {
                (
                    name,
                    AttributeValue::EventHandlerCall {
                        handler: fn_name,
                        args: fn_args,
                    },
                )
            } else {
                (name, AttributeValue::EventHandler(handler))
            }
        } else if let Some(expr) = extract_brace_expr(&value) {
            // title={expr} or title="{expr}" — dynamic binding, expression inside braces.
            (name, AttributeValue::Binding(expr))
        } else {
            (name, AttributeValue::Static(value))
        };

        attrs.push(Attribute {
            name: attr_name,
            value: attr_value,
        });
    }

    Ok(attrs)
}

/// Returns true if the attribute name is an `on...` event handler (`onclick`, `oninput`, etc.).
fn is_event_attr(name: &str) -> bool {
    name.len() > 2 && name.starts_with("on") && name[2..].chars().all(|c| c.is_ascii_alphabetic())
}

/// Parse a handler call like `func(arg1, arg2)` into handler name and argument list.
/// Returns `None` if the value is a simple handler name without call syntax.
fn parse_handler_call(value: &str) -> Option<(String, Vec<String>)> {
    let value = value.trim();
    let open = value.find('(')?;
    if !value.ends_with(')') {
        return None;
    }
    let fn_name = value[..open].trim().to_string();
    if fn_name.is_empty()
        || !fn_name
            .bytes()
            .next()
            .map_or(false, |b| b.is_ascii_alphabetic() || b == b'_')
    {
        return None;
    }
    let args_str = &value[open + 1..value.len() - 1].trim();
    if args_str.is_empty() {
        return Some((fn_name, Vec::new()));
    }
    let args: Vec<String> = split_call_args(args_str)
        .into_iter()
        .map(|a| a.trim().to_string())
        .collect();
    Some((fn_name, args))
}

/// Split comma-separated function call arguments, respecting nested calls and strings.
fn split_call_args(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut quote = b'"';
    let mut start = 0;
    let mut args = Vec::new();

    for i in 0..bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
            continue;
        }
        if b == b'(' {
            depth += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            continue;
        }
        if depth == 0 && b == b',' {
            args.push(&s[start..i]);
            start = i + 1;
        }
    }
    if start < s.len() {
        args.push(&s[start..]);
    }
    args
}

/// If `value` is exactly `{expr}`, returns the inner expression; otherwise `None`.
fn extract_brace_expr(value: &str) -> Option<String> {
    if value.starts_with('{') && value.ends_with('}') && value.len() >= 2 {
        Some(value[1..value.len() - 1].trim().to_string())
    } else {
        None
    }
}

fn parse_inline_nodes(text: &str) -> Vec<TemplateNode> {
    let mut nodes = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next `{expr}` expression.
        let Some(start) = remaining.find('{') else {
            break;
        };

        let prefix = &remaining[..start];
        if !prefix.trim().is_empty() {
            nodes.push(TemplateNode::Text(TextNode {
                content: prefix.trim().to_string(),
            }));
        }

        // `{expr}` — find the matching `}` respecting nested parens so `{t(a.b)}` works.
        let expr_body = &remaining[start + 1..];
        if let Some(end) = find_closing_brace(expr_body) {
            let expr = expr_body[..end].trim();
            if !expr.is_empty() {
                nodes.push(TemplateNode::Expr(ExprNode {
                    expression: expr.to_string(),
                }));
            }
            remaining = &expr_body[end + 1..];
        } else {
            // Unclosed `{` — emit as literal and stop.
            nodes.push(TemplateNode::Text(TextNode {
                content: remaining[start..].to_string(),
            }));
            remaining = "";
        }
    }

    if !remaining.trim().is_empty() {
        nodes.push(TemplateNode::Text(TextNode {
            content: remaining.trim().to_string(),
        }));
    }

    nodes
}

/// Find the index of the `}` that closes the expression, respecting nested
/// parentheses and string literals so `t(a.b)` and `t("key")` are handled.
fn find_closing_brace(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut string_char = '\0';
    let chars = s.char_indices();

    for (i, ch) in chars {
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    return None; // unbalanced
                }
                depth -= 1;
            }
            '}' if depth == 0 => return Some(i),
            _ => {}
        }
    }

    None
}

fn build_template_node(
    tag: String,
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
    imported_components: &HashMap<String, ComponentImportTarget>,
) -> Result<TemplateNode, ParseError> {
    // Control-flow nodes produced by preprocess_control_flow.
    if tag == "mesh-for" {
        let item_name = find_static_attr(&attributes, "item").unwrap_or_default();
        let iterable = find_static_attr(&attributes, "iterable").unwrap_or_default();
        return Ok(TemplateNode::For(ForNode {
            item_name,
            iterable,
            children,
        }));
    }
    if tag == "mesh-if" {
        return Ok(build_if_node(children));
    }
    // mesh-ifthen / mesh-else remain as Element so build_if_node can extract them.
    if tag == "mesh-ifthen" || tag == "mesh-else" {
        return Ok(TemplateNode::Element(ElementNode {
            tag,
            tag_kind: crate::template::SourceTag::Unknown,
            attributes,
            children,
        }));
    }

    if tag == "slot" {
        let name = attributes.iter().find_map(|attribute| {
            if attribute.name != "name" {
                return None;
            }

            match &attribute.value {
                AttributeValue::Static(value) => Some(value.clone()),
                _ => None,
            }
        });

        return Ok(TemplateNode::Slot(SlotNode { name }));
    }

    let tag_kind = crate::template::SourceTag::from_tag_name(&tag);
    if tag_kind != crate::template::SourceTag::Unknown {
        return Ok(TemplateNode::Element(ElementNode {
            tag,
            tag_kind,
            attributes,
            children,
        }));
    }

    if is_reserved_pascal_primitive(&tag) {
        return Err(ParseError::InvalidTemplate {
            message: format!(
                "built-in UI tag <{tag}> must be lowercase; use <{}> instead",
                lowercase_primitive_name(&tag)
            ),
        });
    }

    if tag.chars().next().is_some_and(char::is_uppercase) {
        match imported_components.get(&tag) {
            Some(
                ComponentImportTarget::ComponentLocal(_)
                | ComponentImportTarget::ComponentModule(_),
            ) => {}
            Some(ComponentImportTarget::InterfaceApi { interface, .. }) => {
                return Err(ParseError::InvalidTemplate {
                    message: format!(
                        "component <{tag}> refers to interface import `{interface}`; component tags must use mounted component definitions, not service/interface instances"
                    ),
                });
            }
            None => {
                return Err(ParseError::InvalidTemplate {
                    message: format!(
                        "component <{tag}> is not imported; add `import {tag} from \"...\"` to the script block"
                    ),
                });
            }
        }
        return Ok(TemplateNode::Component(ComponentRef {
            name: tag,
            props: attributes,
            children,
        }));
    }

    Err(ParseError::InvalidTemplate {
        message: format!(
            "unknown UI tag <{tag}>; use lowercase MESH primitives like <box>, <row>, <column>, <text>, <button>, <input>, <text-input>, <slider>, <icon>, or a PascalCase custom component tag"
        ),
    })
}

fn is_reserved_pascal_primitive(tag: &str) -> bool {
    matches!(
        tag,
        "Panel"
            | "Row"
            | "Column"
            | "Grid"
            | "Stack"
            | "ScrollView"
            | "ScrollArea"
            | "Spacer"
            | "Divider"
            | "Separator"
            | "Section"
            | "Header"
            | "Footer"
            | "Group"
            | "FormRow"
            | "Text"
            | "Label"
            | "Icon"
            | "Image"
            | "Badge"
            | "Progress"
            | "Meter"
            | "Tooltip"
            | "Avatar"
            | "Shortcut"
            | "Button"
            | "IconButton"
            | "ToggleButton"
            | "CommandButton"
            | "LinkButton"
            | "Input"
            | "TextArea"
            | "TextInput"
            | "PasswordInput"
            | "SearchInput"
            | "Search"
            | "Password"
            | "NumberInput"
            | "Stepper"
            | "EmailInput"
            | "UrlInput"
            | "Slider"
            | "Select"
            | "Option"
            | "Switch"
            | "Checkbox"
            | "Radio"
            | "RadioGroup"
            | "SegmentedControl"
            | "Menu"
            | "MenuItem"
            | "CommandItem"
            | "PreferenceRow"
            | "Popover"
            | "Dialog"
            | "Sheet"
            | "Tabs"
            | "Tab"
            | "Accordion"
            | "Details"
            | "List"
            | "ListItem"
            | "Table"
            | "Cell"
            | "Tree"
            | "EmptyState"
            | "Slot"
            | "Surface"
            | "Widget"
    )
}

fn lowercase_primitive_name(tag: &str) -> &'static str {
    match tag {
        "ScrollView" => "scroll-view",
        "ScrollArea" => "scroll-area",
        "IconButton" => "icon-button",
        "ToggleButton" => "toggle-button",
        "CommandButton" => "command-button",
        "LinkButton" => "link-button",
        "TextArea" => "textarea",
        "TextInput" => "text-input",
        "PasswordInput" => "password-input",
        "SearchInput" => "search-input",
        "RadioGroup" => "radio-group",
        "SegmentedControl" => "segmented-control",
        "MenuItem" => "menu-item",
        "CommandItem" => "command-item",
        "PreferenceRow" => "preference-row",
        "FormRow" => "form-row",
        "NumberInput" => "number-input",
        "EmptyState" => "empty-state",
        "EmailInput" => "email-input",
        "UrlInput" => "url-input",
        "Grid" => "grid",
        "ListItem" => "list-item",
        "Divider" => "divider",
        "Section" => "section",
        "Header" => "header",
        "Footer" => "footer",
        "Group" => "group",
        "Badge" => "badge",
        "Progress" => "progress",
        "Meter" => "meter",
        "Tooltip" => "tooltip",
        "Avatar" => "avatar",
        "Shortcut" => "shortcut",
        "Search" => "search",
        "Password" => "password",
        "Stepper" => "stepper",
        "Select" => "select",
        "Option" => "option",
        "Radio" => "radio",
        "Menu" => "menu",
        "Popover" => "popover",
        "Dialog" => "dialog",
        "Sheet" => "sheet",
        "Tabs" => "tabs",
        "Tab" => "tab",
        "Accordion" => "accordion",
        "Details" => "details",
        "Table" => "table",
        "Cell" => "cell",
        "Tree" => "tree",
        "Panel" => "panel",
        "Row" => "row",
        "Column" => "column",
        "Stack" => "stack",
        "Spacer" => "spacer",
        "Separator" => "separator",
        "Text" => "text",
        "Label" => "label",
        "Icon" => "icon",
        "Image" => "image",
        "Button" => "button",
        "Input" => "input",
        "Slider" => "slider",
        "Switch" => "switch",
        "Checkbox" => "checkbox",
        "List" => "list",
        "Slot" => "slot",
        "Surface" => "surface",
        "Widget" => "widget",
        _ => "unknown",
    }
}

/// Build a nested `IfNode` tree from the `mesh-ifthen` / `mesh-else` children
/// that `preprocess_control_flow` placed inside a `mesh-if` element.
///
/// Multiple `mesh-ifthen` branches are folded into a chain of nested `IfNode`s
/// so that `{:else if}` is handled correctly.
fn build_if_node(children: Vec<TemplateNode>) -> TemplateNode {
    let mut branches: Vec<(String, Vec<TemplateNode>)> = Vec::new();
    let mut else_children: Vec<TemplateNode> = Vec::new();

    for child in children {
        match child {
            TemplateNode::Element(el) if el.tag == "mesh-ifthen" => {
                let cond = find_static_attr(&el.attributes, "condition").unwrap_or_default();
                branches.push((cond, el.children));
            }
            TemplateNode::Element(el) if el.tag == "mesh-else" => {
                else_children = el.children;
            }
            _ => {}
        }
    }

    if branches.is_empty() {
        return TemplateNode::Element(ElementNode {
            tag: "box".into(),
            tag_kind: crate::template::SourceTag::Box,
            attributes: vec![],
            children: else_children,
        });
    }

    // Fold branches from last to first into a nested IfNode chain.
    let mut current_else = else_children;
    for (cond, then_children) in branches.into_iter().rev() {
        let node = TemplateNode::If(IfNode {
            condition: cond,
            then_children,
            else_children: current_else,
        });
        current_else = vec![node];
    }

    current_else.remove(0)
}

fn find_static_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            AttributeValue::Static(v) => Some(v.clone()),
            _ => None,
        })
}

fn push_template_node(stack: &mut [OpenNode], root: &mut Vec<TemplateNode>, node: TemplateNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root.push(node);
    }
}

fn decode_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

struct OpenNode {
    tag: String,
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_native_tags_parse_as_elements() {
        let template = parse_markup(
            r#"<grid><segmented-control /><empty-state /></grid>"#,
            &HashMap::new(),
        )
        .expect("template parses");

        let TemplateNode::Element(grid) = &template.root[0] else {
            panic!("expected grid element");
        };
        assert_eq!(grid.tag_kind, SourceTag::Grid);
        let TemplateNode::Element(segmented) = &grid.children[0] else {
            panic!("expected segmented-control element");
        };
        assert_eq!(segmented.tag_kind, SourceTag::SegmentedControl);
        let TemplateNode::Element(empty_state) = &grid.children[1] else {
            panic!("expected empty-state element");
        };
        assert_eq!(empty_state.tag_kind, SourceTag::EmptyState);
    }

    #[test]
    fn reserved_pascal_primitives_report_lowercase_element_names() {
        let err = parse_markup("<SegmentedControl />", &HashMap::new())
            .expect_err("PascalCase primitive should be rejected")
            .to_string();

        assert!(err.contains("built-in UI tag <SegmentedControl> must be lowercase"));
        assert!(err.contains("<segmented-control>"));
    }
}
