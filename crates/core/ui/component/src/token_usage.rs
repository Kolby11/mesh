use crate::{
    Attribute, AttributeValue, ComponentFile, PropType, PropValue, ScriptStringCall, SourceSpan,
    TemplateNode,
};
use mesh_core_theme::{canonical_token_name, css_token_usage, is_private_token, is_standard_token};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeTokenRead {
    pub name: String,
    pub span: SourceSpan,
}

/// Static reads outside the semantic vocabulary and the component/module's
/// own declarations. Dynamic token names are left to runtime resolution.
pub fn unknown_theme_token_reads(
    component: &ComponentFile,
    source: &str,
    local_tokens: &[String],
) -> Vec<ThemeTokenRead> {
    let mut locals = local_tokens
        .iter()
        .map(|name| canonical_token_name(name))
        .collect::<BTreeSet<_>>();
    let mut reads = Vec::new();
    if let Some(style) = &component.style {
        if let Some(css) = source.get(style.span.start..style.span.end) {
            let usage = css_token_usage(css);
            locals.extend(usage.declarations);
            reads.extend(usage.references.into_iter().map(|read| ThemeTokenRead {
                name: read.name,
                span: SourceSpan::new(
                    style.span.start + read.span.start,
                    style.span.start + read.span.end,
                ),
            }));
        }
    }
    let mut callees = BTreeSet::from(["mesh.theme.token".to_owned()]);
    if let Some(script) = &component.script {
        for (alias, interface) in &script.metadata.interface_proxies {
            if interface == "mesh.theme" {
                callees.insert(format!("{alias}.token"));
            }
        }
        collect_calls(
            &script.metadata.string_calls,
            script.span.start,
            &callees,
            &mut reads,
        );
    }
    if let Some(props) = &component.props {
        for prop in &props.props {
            if let Some(PropValue::String(value)) = &prop.default {
                if matches!(prop.ty, PropType::Token | PropType::Color | PropType::Size) {
                    let css = crate::prop_value_to_css(prop, &PropValue::String(value.clone()))
                        .unwrap_or_else(|_| value.clone());
                    reads.extend(css_token_usage(&css).references.into_iter().map(|read| {
                        ThemeTokenRead {
                            name: read.name,
                            span: prop.span,
                        }
                    }));
                }
            }
        }
    }
    if let Some(template) = &component.template {
        walk_nodes(&template.root, source, &callees, &mut locals, &mut reads);
    }
    reads.retain(|read| {
        !is_standard_token(&read.name)
            && !is_private_token(&read.name)
            && !locals.contains(&canonical_token_name(&read.name))
    });
    reads
        .sort_by(|left, right| (left.span.start, &left.name).cmp(&(right.span.start, &right.name)));
    reads.dedup();
    reads
}

fn collect_calls(
    calls: &[ScriptStringCall],
    base: usize,
    callees: &BTreeSet<String>,
    reads: &mut Vec<ThemeTokenRead>,
) {
    reads.extend(
        calls
            .iter()
            .filter(|call| callees.contains(&call.callee))
            .map(|call| ThemeTokenRead {
                name: call.argument.clone(),
                span: SourceSpan::new(base + call.span.start, base + call.span.end),
            }),
    );
}

fn expression(
    source: &str,
    authored: &str,
    span: SourceSpan,
    callees: &BTreeSet<String>,
    reads: &mut Vec<ThemeTokenRead>,
) {
    if !source.contains("token") {
        return;
    }
    let calls = crate::parser::string_calls_in_expression(source);
    if let Some(offset) = authored
        .get(span.start..span.end)
        .and_then(|body| body.find(source))
    {
        collect_calls(&calls, span.start + offset, callees, reads);
    } else {
        reads.extend(
            calls
                .into_iter()
                .filter(|call| callees.contains(&call.callee))
                .map(|call| ThemeTokenRead {
                    name: call.argument,
                    span,
                }),
        );
    }
}

fn attributes(
    attrs: &[Attribute],
    source: &str,
    callees: &BTreeSet<String>,
    locals: &mut BTreeSet<String>,
    reads: &mut Vec<ThemeTokenRead>,
) {
    for attr in attrs {
        let span = attr.span.unwrap_or_default();
        if let Some(name) = attr
            .name
            .strip_prefix("style:")
            .filter(|name| name.starts_with("--"))
        {
            locals.insert(canonical_token_name(name));
        }
        match &attr.value {
            AttributeValue::Static(value)
                if attr.name == "style" || attr.name.starts_with("style:") =>
            {
                let usage = css_token_usage(value);
                locals.extend(usage.declarations);
                reads.extend(usage.references.into_iter().map(|read| ThemeTokenRead {
                    name: read.name,
                    span,
                }));
            }
            AttributeValue::Binding(value)
            | AttributeValue::TwoWayBinding(value)
            | AttributeValue::EventHandlerExpression { source: value, .. } => {
                expression(value, source, span, callees, reads)
            }
            _ => {}
        }
    }
}

fn walk_nodes(
    nodes: &[TemplateNode],
    source: &str,
    callees: &BTreeSet<String>,
    locals: &mut BTreeSet<String>,
    reads: &mut Vec<ThemeTokenRead>,
) {
    for node in nodes {
        match node {
            TemplateNode::Element(node) => {
                attributes(&node.attributes, source, callees, locals, reads);
                walk_nodes(&node.children, source, callees, locals, reads);
            }
            TemplateNode::Component(node) => {
                attributes(&node.props, source, callees, locals, reads);
                walk_nodes(&node.children, source, callees, locals, reads);
            }
            TemplateNode::Expr(node) => expression(
                &node.expression,
                source,
                node.expression_span,
                callees,
                reads,
            ),
            TemplateNode::If(node) => {
                expression(&node.condition, source, node.condition_span, callees, reads);
                walk_nodes(&node.then_children, source, callees, locals, reads);
                walk_nodes(&node.else_children, source, callees, locals, reads);
            }
            TemplateNode::For(node) => {
                expression(&node.iterable, source, node.iterable_span, callees, reads);
                if let Some(key) = &node.key {
                    expression(
                        key,
                        source,
                        node.key_span.unwrap_or(node.span),
                        callees,
                        reads,
                    );
                }
                walk_nodes(&node.children, source, callees, locals, reads);
            }
            TemplateNode::Text(_) | TemplateNode::Slot(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_share_ast_and_css_semantics_with_authoring_spans() {
        let source = r#"<template><text>{theme.token("color.template-typo")}</text></template>
<style>.box { --local: 2; width: var(--local); color: var(--color-typo); padding: var(--spacing-md); }</style>
<script lang="luau">
local theme = require("mesh.theme")
local value = mesh.theme.token("color.\x74ypo")
local private = theme.token("@alice/mod.color.private")
local text = 'mesh.theme.token("color.fake")'
-- mesh.theme.token("color.comment")
</script>"#;
        let component = crate::parse_component(source).unwrap();
        let reads = unknown_theme_token_reads(&component, source, &[]);
        assert_eq!(
            reads
                .iter()
                .map(|read| read.name.as_str())
                .collect::<Vec<_>>(),
            ["color.template-typo", "--color-typo", "color.typo"]
        );
        assert!(
            reads
                .iter()
                .all(|read| read.span.start < read.span.end && read.span.end <= source.len())
        );
        assert_eq!(
            unknown_theme_token_reads(&component, source, &["color.typo".into()]).len(),
            1
        );
    }

    #[test]
    fn css_capable_prop_defaults_are_read_through_their_projection() {
        let source = r#"<props>
{
    accent = { type = "token", default = "color.primary" },
    typo = { type = "token", default = "color-acent" },
    surface = { type = "color", default = "var(--color-surfce, #000)" },
    gap = { type = "size", default = "var(--spacing-md)" },
    label = { type = "string", default = "var(--not-a-read)" },
    own = { type = "token", default = "local-accent" },
}
</props>
<template><box /></template>
<style>.box { --local-accent: red; color: prop(own); }</style>"#;
        let component = crate::parse_component(source).unwrap();
        let reads = unknown_theme_token_reads(&component, source, &[]);
        assert_eq!(
            reads
                .iter()
                .map(|read| read.name.as_str())
                .collect::<Vec<_>>(),
            ["--color-acent", "--color-surfce"]
        );
        let props = &component.props.as_ref().unwrap().props;
        assert_eq!(reads[0].span, props[1].span);
        assert_eq!(reads[1].span, props[2].span);
    }
}
