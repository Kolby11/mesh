use crate::template::*;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

use super::ParseError;
use super::brace::{self, BraceKind, BraceLex};
use crate::{ComponentImportTarget, SourceSpan};

/// Escape special XML attribute characters in synthetic lowering attributes.
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

fn preprocess_template(source: &str, braces: &BraceLex) -> Result<String, ParseError> {
    #[derive(Clone, Copy)]
    enum Branch {
        Then,
        Else,
    }
    enum Flow {
        If(Branch),
        For,
    }

    let mut output = String::with_capacity(source.len() + 128);
    let mut source_cursor = 0usize;
    let mut markup = MarkupState::default();
    let mut flow = Vec::<Flow>::new();

    for (id, token) in braces.tokens.iter().enumerate() {
        let unchanged = &source[source_cursor..token.span.start];
        output.push_str(unchanged);
        markup.advance(unchanged);

        match &token.kind {
            BraceKind::Expression { .. } if markup.in_tag => {
                if markup.quote.is_none()
                    && source[..token.span.start]
                        .chars()
                        .rev()
                        .find(|ch| !ch.is_whitespace())
                        .is_none_or(|ch| ch != '=')
                {
                    return Err(ParseError::InvalidTemplate {
                        message: "attribute interpolation must follow `=`".into(),
                    });
                }
                let marker = BraceLex::marker(id);
                if markup.quote.is_some() {
                    output.push_str(&marker);
                } else {
                    output.push('"');
                    output.push_str(&marker);
                    output.push('"');
                }
            }
            BraceKind::Expression { .. } => {
                output.push_str(&format!("<mesh-expr data-mesh-id=\"{id}\" />"));
            }
            BraceKind::IfOpen { condition } => {
                if markup.in_tag {
                    return Err(ParseError::InvalidTemplate {
                        message: "control-flow directives cannot appear inside a tag".into(),
                    });
                }
                let close_end = token.matching_end.expect("validated control-flow match");
                let condition = xml_attr_escape(&source[condition.start..condition.end]);
                output.push_str(&format!(
                    "<mesh-if data-mesh-id=\"{id}\" data-mesh-end=\"{close_end}\"><mesh-ifthen data-mesh-condition-id=\"{id}\" condition=\"{condition}\">"
                ));
                flow.push(Flow::If(Branch::Then));
            }
            BraceKind::ForOpen {
                item,
                iterable,
                key,
            } => {
                if markup.in_tag {
                    return Err(ParseError::InvalidTemplate {
                        message: "control-flow directives cannot appear inside a tag".into(),
                    });
                }
                let close_end = token.matching_end.expect("validated control-flow match");
                let key = key
                    .map(|key| format!(" key=\"{}\"", xml_attr_escape(&source[key.start..key.end])))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "<mesh-for data-mesh-id=\"{id}\" data-mesh-end=\"{close_end}\" item=\"{}\" iterable=\"{}\"{}>",
                    xml_attr_escape(item),
                    xml_attr_escape(&source[iterable.start..iterable.end]),
                    key,
                ));
                flow.push(Flow::For);
            }
            BraceKind::ElseIf { condition } => {
                let Some(Flow::If(branch)) = flow.last_mut() else {
                    unreachable!("brace parser validates else-if nesting")
                };
                output.push_str(match branch {
                    Branch::Then => "</mesh-ifthen>",
                    Branch::Else => "</mesh-else>",
                });
                let condition = xml_attr_escape(&source[condition.start..condition.end]);
                output.push_str(&format!(
                    "<mesh-ifthen data-mesh-condition-id=\"{id}\" condition=\"{condition}\">"
                ));
                *branch = Branch::Then;
            }
            BraceKind::Else => {
                let Some(Flow::If(branch)) = flow.last_mut() else {
                    unreachable!("brace parser validates else nesting")
                };
                output.push_str(match branch {
                    Branch::Then => "</mesh-ifthen>",
                    Branch::Else => "</mesh-else>",
                });
                output.push_str("<mesh-else>");
                *branch = Branch::Else;
            }
            BraceKind::IfClose => {
                let Some(Flow::If(branch)) = flow.pop() else {
                    unreachable!("brace parser validates if closing")
                };
                output.push_str(match branch {
                    Branch::Then => "</mesh-ifthen></mesh-if>",
                    Branch::Else => "</mesh-else></mesh-if>",
                });
            }
            BraceKind::ForClose => {
                let Some(Flow::For) = flow.pop() else {
                    unreachable!("brace parser validates for closing")
                };
                output.push_str("</mesh-for>");
            }
        }
        source_cursor = token.span.end;
    }

    output.push_str(&source[source_cursor..]);
    if !flow.is_empty() {
        unreachable!("brace parser validates all control-flow closures")
    }
    Ok(output)
}

#[derive(Default)]
struct MarkupState {
    in_tag: bool,
    quote: Option<char>,
}

impl MarkupState {
    fn advance(&mut self, source: &str) {
        for ch in source.chars() {
            if let Some(quote) = self.quote {
                if ch == quote {
                    self.quote = None;
                }
                continue;
            }
            match ch {
                '<' if !self.in_tag => self.in_tag = true,
                '>' if self.in_tag => self.in_tag = false,
                '\'' | '"' if self.in_tag => self.quote = Some(ch),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
pub(super) fn parse_markup(
    source: &str,
    imported_components: &HashMap<String, ComponentImportTarget>,
) -> Result<TemplateBlock, ParseError> {
    parse_markup_at(source, 0, imported_components)
}

pub(super) fn parse_markup_at(
    source: &str,
    source_base: usize,
    imported_components: &HashMap<String, ComponentImportTarget>,
) -> Result<TemplateBlock, ParseError> {
    let braces = brace::lex(source)?;
    let preprocessed = preprocess_template(source, &braces)?;
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
                let attrs = parse_xml_attributes(source, &reader, &event, &braces, source_base)?;
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
                let attrs = parse_xml_attributes(source, &reader, &event, &braces, source_base)?;
                let node = build_template_node(
                    tag,
                    attrs,
                    Vec::new(),
                    imported_components,
                    source,
                    &braces,
                    source_base,
                )?;
                push_template_node(&mut stack, &mut root, node);
            }
            Ok(Event::Text(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                if !text.trim().is_empty() {
                    push_template_node(
                        &mut stack,
                        &mut root,
                        TemplateNode::Text(TextNode {
                            content: text.trim().to_string(),
                        }),
                    );
                }
            }
            Ok(Event::CData(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                if !text.trim().is_empty() {
                    push_template_node(
                        &mut stack,
                        &mut root,
                        TemplateNode::Text(TextNode {
                            content: text.trim().to_string(),
                        }),
                    );
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
                    source,
                    &braces,
                    source_base,
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

    assign_duplicate_component_ordinals(&mut root);
    Ok(TemplateBlock { root })
}

fn visit_component_refs(nodes: &[TemplateNode], visit: &mut impl FnMut(&ComponentRef)) {
    for node in nodes {
        match node {
            TemplateNode::Component(component) => {
                visit(component);
                visit_component_refs(&component.children, visit);
            }
            TemplateNode::Element(element) => visit_component_refs(&element.children, visit),
            TemplateNode::If(if_node) => {
                visit_component_refs(&if_node.then_children, visit);
                visit_component_refs(&if_node.else_children, visit);
            }
            TemplateNode::For(for_node) => visit_component_refs(&for_node.children, visit),
            TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
        }
    }
}

fn visit_component_refs_mut(nodes: &mut [TemplateNode], visit: &mut impl FnMut(&mut ComponentRef)) {
    for node in nodes {
        match node {
            TemplateNode::Component(component) => {
                visit(component);
                visit_component_refs_mut(&mut component.children, visit);
            }
            TemplateNode::Element(element) => {
                visit_component_refs_mut(&mut element.children, visit);
            }
            TemplateNode::If(if_node) => {
                visit_component_refs_mut(&mut if_node.then_children, visit);
                visit_component_refs_mut(&mut if_node.else_children, visit);
            }
            TemplateNode::For(for_node) => {
                visit_component_refs_mut(&mut for_node.children, visit);
            }
            TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
        }
    }
}

fn assign_duplicate_component_ordinals(nodes: &mut [TemplateNode]) {
    let mut counts = HashMap::<String, usize>::new();
    visit_component_refs(nodes, &mut |component| {
        *counts.entry(component.name.clone()).or_default() += 1;
    });

    let mut seen = HashMap::<String, usize>::new();
    let mut source_ordinal = 0usize;
    visit_component_refs_mut(nodes, &mut |component| {
        component.source_ordinal = source_ordinal;
        source_ordinal += 1;
        if counts.get(&component.name).copied().unwrap_or_default() <= 1 {
            return;
        }
        let ordinal = seen.entry(component.name.clone()).or_default();
        component.duplicate_ordinal = Some(*ordinal);
        *ordinal += 1;
    });
    mark_loop_component_refs(nodes, false);
}

fn mark_loop_component_refs(nodes: &mut [TemplateNode], inside_loop: bool) {
    for node in nodes {
        match node {
            TemplateNode::Component(component) => {
                component.repeated_by_loop = inside_loop;
                mark_loop_component_refs(&mut component.children, inside_loop);
            }
            TemplateNode::Element(element) => {
                mark_loop_component_refs(&mut element.children, inside_loop);
            }
            TemplateNode::If(if_node) => {
                mark_loop_component_refs(&mut if_node.then_children, inside_loop);
                mark_loop_component_refs(&mut if_node.else_children, inside_loop);
            }
            TemplateNode::For(for_node) => mark_loop_component_refs(&mut for_node.children, true),
            TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
        }
    }
}

fn parse_xml_attributes(
    source: &str,
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    braces: &BraceLex,
    source_base: usize,
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

        let expression = BraceLex::marker_id(&value).and_then(|id| {
            let token = braces.token(id)?;
            let BraceKind::Expression { expression } = &token.kind else {
                return None;
            };
            Some((
                source[expression.start..expression.end].to_string(),
                crate::SourceSpan::new(
                    source_base + token.span.start,
                    source_base + token.span.end,
                ),
            ))
        });
        let expression_span = expression.as_ref().map(|(_, span)| *span);
        let binding = expression.as_ref().map(|(value, _)| value.as_str());

        let (attr_name, attr_value) = if name == "bind:this" {
            let binding = binding.unwrap_or(value.as_str()).to_string();
            (name, AttributeValue::InstanceBinding(binding))
        } else if let Some(var) = name.strip_prefix("bind:") {
            // bind:value="variable" — two-way binding.
            (
                var.to_string(),
                AttributeValue::TwoWayBinding(binding.unwrap_or(value.as_str()).to_string()),
            )
        } else if is_event_attr(&name) {
            // onclick={handler}, onclick="handler", or onclick="{handler}" — strip braces if present.
            let handler = binding.unwrap_or(value.as_str()).to_string();
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
        } else if let Some((expr, _)) = expression.as_ref() {
            // title={expr} or title="{expr}" — dynamic binding, expression inside braces.
            (name, AttributeValue::Binding(expr.clone()))
        } else {
            (name, AttributeValue::Static(value))
        };

        attrs.push(Attribute {
            name: attr_name,
            value: attr_value,
            span: expression_span,
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

fn build_template_node(
    tag: String,
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
    imported_components: &HashMap<String, ComponentImportTarget>,
    source: &str,
    braces: &BraceLex,
    source_base: usize,
) -> Result<TemplateNode, ParseError> {
    if tag == "mesh-expr" {
        let id = find_static_attr(&attributes, "data-mesh-id")
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| ParseError::InvalidTemplate {
                message: "synthetic interpolation is missing its token id".into(),
            })?;
        let token = braces
            .token(id)
            .ok_or_else(|| ParseError::InvalidTemplate {
                message: "synthetic interpolation has an unknown token id".into(),
            })?;
        let BraceKind::Expression { expression } = &token.kind else {
            return Err(ParseError::InvalidTemplate {
                message: "synthetic interpolation points to a control-flow token".into(),
            });
        };
        return Ok(TemplateNode::Expr(ExprNode {
            expression: source[expression.start..expression.end].to_string(),
            span: add_base(token.span, source_base),
            expression_span: add_base(*expression, source_base),
        }));
    }

    // Control-flow nodes produced by the brace lexer.
    if tag == "mesh-for" {
        let item_name = find_static_attr(&attributes, "item").unwrap_or_default();
        let iterable_text = find_static_attr(&attributes, "iterable").unwrap_or_default();
        let id = synthetic_token_id(&attributes, "data-mesh-id")?;
        let token = braces
            .token(id)
            .ok_or_else(|| ParseError::InvalidTemplate {
                message: "synthetic for-loop has an unknown token id".into(),
            })?;
        let BraceKind::ForOpen { iterable, key, .. } = &token.kind else {
            return Err(ParseError::InvalidTemplate {
                message: "synthetic for-loop points to a non-for token".into(),
            });
        };
        let close_end = synthetic_end(&attributes, "data-mesh-end")?;
        return Ok(TemplateNode::For(ForNode {
            item_name,
            iterable: iterable_text,
            span: SourceSpan::new(source_base + token.span.start, source_base + close_end),
            iterable_span: add_base(*iterable, source_base),
            key: find_static_attr(&attributes, "key"),
            key_span: key.map(|key| add_base(key, source_base)),
            children,
        }));
    }
    if tag == "mesh-if" {
        return Ok(build_if_node(
            attributes,
            children,
            source,
            braces,
            source_base,
        ));
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
        let static_slot_attr = |name: &str| -> Result<Option<String>, ParseError> {
            let Some(attribute) = attributes.iter().find(|attribute| attribute.name == name) else {
                return Ok(None);
            };
            match &attribute.value {
                AttributeValue::Static(value) => Ok(Some(value.clone())),
                _ => Err(ParseError::InvalidTemplate {
                    message: format!("<slot> attribute '{name}' must be static"),
                }),
            }
        };
        let extension_point = static_slot_attr("extension-point")?;
        let name = static_slot_attr("name")?;
        let mode = static_slot_attr("mode")?;
        let customizable = match mode.as_deref().unwrap_or("automatic") {
            "automatic" => false,
            "customizable" => true,
            other => {
                return Err(ParseError::InvalidTemplate {
                    message: format!(
                        "<slot> mode must be 'automatic' or 'customizable', got '{other}'"
                    ),
                });
            }
        };
        if customizable && name.as_ref().is_none_or(|name| name.trim().is_empty()) {
            return Err(ParseError::InvalidTemplate {
                message: "a customizable <slot> requires a non-empty static name".into(),
            });
        }

        return Ok(TemplateNode::Slot(SlotNode {
            extension_point,
            name,
            customizable,
        }));
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
            source_ordinal: 0,
            duplicate_ordinal: None,
            repeated_by_loop: false,
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
fn build_if_node(
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
    _source: &str,
    braces: &BraceLex,
    source_base: usize,
) -> TemplateNode {
    let outer_id = synthetic_token_id(&attributes, "data-mesh-id").unwrap_or_default();
    let outer = braces.token(outer_id);
    let outer_span = outer
        .and_then(|token| {
            token
                .matching_end
                .map(|end| SourceSpan::new(token.span.start, end))
        })
        .map(|span| add_base(span, source_base))
        .unwrap_or_else(|| SourceSpan::new(source_base, source_base));

    let mut branches: Vec<(String, SourceSpan, Vec<TemplateNode>)> = Vec::new();
    let mut else_children: Vec<TemplateNode> = Vec::new();

    for child in children {
        match child {
            TemplateNode::Element(el) if el.tag == "mesh-ifthen" => {
                let cond = find_static_attr(&el.attributes, "condition").unwrap_or_default();
                let condition_id =
                    synthetic_token_id(&el.attributes, "data-mesh-condition-id").ok();
                let condition_span = condition_id
                    .and_then(|id| braces.token(id))
                    .and_then(|token| match &token.kind {
                        BraceKind::IfOpen { condition } | BraceKind::ElseIf { condition } => {
                            Some(add_base(*condition, source_base))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| SourceSpan::new(source_base, source_base));
                branches.push((cond, condition_span, el.children));
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
    for (cond, condition_span, then_children) in branches.into_iter().rev() {
        let node = TemplateNode::If(IfNode {
            condition: cond,
            span: outer_span,
            condition_span,
            then_children,
            else_children: current_else,
        });
        current_else = vec![node];
    }

    current_else.remove(0)
}

fn synthetic_token_id(attrs: &[Attribute], name: &str) -> Result<usize, ParseError> {
    find_static_attr(attrs, name)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| ParseError::InvalidTemplate {
            message: format!("synthetic node is missing `{name}`"),
        })
}

fn synthetic_end(attrs: &[Attribute], name: &str) -> Result<usize, ParseError> {
    synthetic_token_id(attrs, name)
}

fn add_base(span: SourceSpan, base: usize) -> SourceSpan {
    SourceSpan::new(base + span.start, base + span.end)
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
    fn template_preprocessing_preserves_utf8_text_and_attributes() {
        let template = parse_markup(
            r#"<text aria-label="Slovenčina">Slovenčina</text>"#,
            &HashMap::new(),
        )
        .expect("Unicode template parses");

        let TemplateNode::Element(text) = &template.root[0] else {
            panic!("expected text element");
        };
        assert_eq!(
            find_static_attr(&text.attributes, "aria-label").as_deref(),
            Some("Slovenčina")
        );
        let [TemplateNode::Text(content)] = text.children.as_slice() else {
            panic!("expected literal text child");
        };
        assert_eq!(content.content, "Slovenčina");
    }

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
