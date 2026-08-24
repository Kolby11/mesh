/// Parser for `.mesh` single-file components.
///
/// Parses the source into validated top-level blocks (`<template>`, `<script>`, `<style>`,
mod brace;
mod markup;
mod props;
mod script;
mod semantic;
mod styles;

use crate::{
    BlockAttribute, ComponentBlock, ComponentFile, ComponentImportTarget, ParseDiagnosticCategory,
    SourceSpan,
};
use props::parse_props_at;
use script::parse_script;
pub use script::referenced_identifiers;
use std::collections::{HashMap, HashSet};
use styles::parse_style;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unclosed block <{tag}> at {span:?}")]
    UnclosedBlock { tag: String, span: SourceSpan },

    #[error("unexpected closing tag </{tag}> at {span:?}")]
    UnexpectedClose { tag: String, span: SourceSpan },

    #[error("missing required block <{name}>")]
    MissingRequiredBlock { name: String, span: SourceSpan },

    #[error("duplicate block <{name}> at {span:?}")]
    DuplicateBlock { name: String, span: SourceSpan },

    #[error("invalid attributes on <{name}> at {span:?}: {message}")]
    InvalidBlockAttributes {
        name: String,
        message: String,
        span: SourceSpan,
    },

    #[error("unsupported script language `{language}` at {span:?}; expected `luau`")]
    UnsupportedScriptLanguage { language: String, span: SourceSpan },

    #[error("unexpected top-level content at {span:?}: {message}")]
    UnexpectedTopLevelContent { message: String, span: SourceSpan },

    #[error("malformed top-level block at {span:?}: {message}")]
    MalformedTopLevelBlock { message: String, span: SourceSpan },

    #[error("invalid template syntax at {span:?}: {message}")]
    InvalidTemplate { message: String, span: SourceSpan },

    #[error("invalid style syntax at {span:?}: {message}")]
    InvalidStyle { message: String, span: SourceSpan },

    #[error("invalid props block at {span:?}: {message}")]
    InvalidProps { message: String, span: SourceSpan },

    #[error("invalid component semantics at {span:?}: {message}")]
    InvalidSemantics { message: String, span: SourceSpan },

    #[error("invalid i18n block at {span:?}: {message}")]
    InvalidI18n { message: String, span: SourceSpan },

    #[error("invalid import at {span:?}: {message}")]
    InvalidImport { message: String, span: SourceSpan },

    #[error("unknown block <{name}> at {span:?}")]
    UnknownBlock { name: String, span: SourceSpan },
}

impl ParseError {
    /// Return the stable category retained by compiler and editor adapters.
    pub const fn category(&self) -> ParseDiagnosticCategory {
        match self {
            Self::UnclosedBlock { .. }
            | Self::UnexpectedClose { .. }
            | Self::MissingRequiredBlock { .. }
            | Self::DuplicateBlock { .. }
            | Self::InvalidBlockAttributes { .. }
            | Self::UnsupportedScriptLanguage { .. }
            | Self::UnexpectedTopLevelContent { .. }
            | Self::MalformedTopLevelBlock { .. }
            | Self::UnknownBlock { .. } => ParseDiagnosticCategory::Syntax,
            Self::InvalidTemplate { .. } => ParseDiagnosticCategory::Template,
            Self::InvalidStyle { .. } => ParseDiagnosticCategory::Style,
            Self::InvalidProps { .. } => ParseDiagnosticCategory::Props,
            Self::InvalidSemantics { .. } => ParseDiagnosticCategory::Semantics,
            Self::InvalidI18n { .. } => ParseDiagnosticCategory::I18n,
            Self::InvalidImport { .. } => ParseDiagnosticCategory::Import,
        }
    }

    /// The source range owned by the diagnostic. A missing template has no
    /// offending token and is intentionally represented by an empty range.
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::MissingRequiredBlock { span, .. } => *span,
            Self::UnclosedBlock { span, .. }
            | Self::UnexpectedClose { span, .. }
            | Self::DuplicateBlock { span, .. }
            | Self::InvalidBlockAttributes { span, .. }
            | Self::UnsupportedScriptLanguage { span, .. }
            | Self::UnexpectedTopLevelContent { span, .. }
            | Self::MalformedTopLevelBlock { span, .. }
            | Self::InvalidTemplate { span, .. }
            | Self::InvalidStyle { span, .. }
            | Self::InvalidProps { span, .. }
            | Self::InvalidSemantics { span, .. }
            | Self::InvalidI18n { span, .. }
            | Self::InvalidImport { span, .. }
            | Self::UnknownBlock { span, .. } => *span,
        }
    }

    /// Rebase a parser-local span into the owning component source.
    pub(crate) fn with_base(self, base: usize) -> Self {
        self.map_span(|span| SourceSpan::new(base + span.start, base + span.end))
    }

    /// Give internally generated diagnostics a reliable owning-block range
    /// when the underlying library did not report a token location.
    pub(crate) fn with_fallback(self, fallback: SourceSpan) -> Self {
        if self.span().len() != 0 {
            self
        } else {
            self.map_span(|_| fallback)
        }
    }

    fn map_span(self, map: impl Fn(SourceSpan) -> SourceSpan) -> Self {
        match self {
            Self::UnclosedBlock { tag, span } => Self::UnclosedBlock {
                tag,
                span: map(span),
            },
            Self::UnexpectedClose { tag, span } => Self::UnexpectedClose {
                tag,
                span: map(span),
            },
            Self::MissingRequiredBlock { name, span } => Self::MissingRequiredBlock {
                name,
                span: map(span),
            },
            Self::DuplicateBlock { name, span } => Self::DuplicateBlock {
                name,
                span: map(span),
            },
            Self::InvalidBlockAttributes {
                name,
                message,
                span,
            } => Self::InvalidBlockAttributes {
                name,
                message,
                span: map(span),
            },
            Self::UnsupportedScriptLanguage { language, span } => Self::UnsupportedScriptLanguage {
                language,
                span: map(span),
            },
            Self::UnexpectedTopLevelContent { message, span } => Self::UnexpectedTopLevelContent {
                message,
                span: map(span),
            },
            Self::MalformedTopLevelBlock { message, span } => Self::MalformedTopLevelBlock {
                message,
                span: map(span),
            },
            Self::InvalidTemplate { message, span } => Self::InvalidTemplate {
                message,
                span: map(span),
            },
            Self::InvalidStyle { message, span } => Self::InvalidStyle {
                message,
                span: map(span),
            },
            Self::InvalidProps { message, span } => Self::InvalidProps {
                message,
                span: map(span),
            },
            Self::InvalidSemantics { message, span } => Self::InvalidSemantics {
                message,
                span: map(span),
            },
            Self::InvalidI18n { message, span } => Self::InvalidI18n {
                message,
                span: map(span),
            },
            Self::InvalidImport { message, span } => Self::InvalidImport {
                message,
                span: map(span),
            },
            Self::UnknownBlock { name, span } => Self::UnknownBlock {
                name,
                span: map(span),
            },
        }
    }
}

pub fn parse_component(source: &str) -> Result<ComponentFile, ParseError> {
    parse_component_impl(source, true)
}

/// Parse the structural component tree for editor tooling while allowing
/// incomplete cross-block references to remain available for completion.
///
/// Compiler and runtime callers must use [`parse_component`], which runs the
/// semantic contract pass before accepting the component.
pub fn parse_component_for_tooling(source: &str) -> Result<ComponentFile, ParseError> {
    parse_component_impl(source, false)
}

fn parse_component_impl(
    source: &str,
    validate_semantics: bool,
) -> Result<ComponentFile, ParseError> {
    let blocks = parse_top_level_blocks(source)?;

    let (imports, script) = if let Some(block) = blocks.iter().find(|block| block.name == "script")
    {
        let (mut imports, mut script) =
            parse_script(&source[block.content.start..block.content.end])
                .map_err(|error| error.with_base(block.content.start))?;
        script.span = SourceSpan::new(block.content.start, block.content.end);
        for import in &mut imports {
            import.span = add_base(import.span, block.content.start);
            import.alias_span = add_base(import.alias_span, block.content.start);
            import.target_span = add_base(import.target_span, block.content.start);
        }
        (imports, Some(script))
    } else {
        (Vec::new(), None)
    };
    let imported_components: HashMap<String, ComponentImportTarget> = imports
        .iter()
        .map(|import| (import.alias.clone(), import.target.clone()))
        .collect();

    let template = blocks
        .iter()
        .find(|block| block.name == "template")
        .map(|block| {
            markup::parse_markup_at(
                &source[block.content.start..block.content.end],
                block.content.start,
                &imported_components,
            )
        })
        .transpose()?;

    let style = blocks
        .iter()
        .find(|block| block.name == "style")
        .map(|block| {
            parse_style(
                &source[block.content.start..block.content.end],
                block.content.start,
            )
        })
        .transpose()?;

    let props = blocks
        .iter()
        .find(|block| block.name == "props")
        .map(|block| {
            parse_props_at(
                &source[block.content.start..block.content.end],
                block.content.start,
            )
        })
        .transpose()?;

    let template_expressions = compile_template_expressions(template.as_ref())?;

    let component = ComponentFile {
        blocks,
        imports,
        props,
        template,
        script,
        style,
        template_expressions,
    };
    if validate_semantics {
        semantic::validate(source, &component)?;
    }
    Ok(component)
}

fn compile_template_expressions(
    template: Option<&crate::TemplateBlock>,
) -> Result<Vec<crate::SharedCompiledExpression>, ParseError> {
    use crate::template::{AttributeValue, TemplateNode};

    fn compile(
        source: &str,
        span: SourceSpan,
        seen: &mut HashSet<String>,
        expressions: &mut Vec<crate::SharedCompiledExpression>,
    ) -> Result<(), ParseError> {
        let normalized = source.trim();
        if !seen.insert(normalized.to_owned()) {
            return Ok(());
        }
        let expression =
            crate::compile_expression(source).map_err(|error| ParseError::InvalidTemplate {
                message: format!("invalid Luau expression: {error}"),
                span,
            })?;
        expressions.push(expression);
        Ok(())
    }

    fn attributes(
        attributes: &[crate::template::Attribute],
        seen: &mut HashSet<String>,
        expressions: &mut Vec<crate::SharedCompiledExpression>,
    ) -> Result<(), ParseError> {
        for attribute in attributes {
            match &attribute.value {
                AttributeValue::Binding(expression) | AttributeValue::TwoWayBinding(expression) => {
                    compile(
                        expression,
                        attribute.span.unwrap_or_default(),
                        seen,
                        expressions,
                    )?;
                }
                AttributeValue::EventHandlerCall { args, .. } => {
                    for expression in args {
                        compile(
                            expression,
                            attribute.span.unwrap_or_default(),
                            seen,
                            expressions,
                        )?;
                    }
                }
                AttributeValue::Static(_)
                | AttributeValue::InstanceBinding(_)
                | AttributeValue::EventHandler(_) => {}
            }
        }
        Ok(())
    }

    fn nodes(
        template_nodes: &[TemplateNode],
        seen: &mut HashSet<String>,
        expressions: &mut Vec<crate::SharedCompiledExpression>,
    ) -> Result<(), ParseError> {
        for node in template_nodes {
            match node {
                TemplateNode::Element(element) => {
                    attributes(&element.attributes, seen, expressions)?;
                    nodes(&element.children, seen, expressions)?;
                }
                TemplateNode::Component(component) => {
                    attributes(&component.props, seen, expressions)?;
                    nodes(&component.children, seen, expressions)?;
                }
                TemplateNode::Expr(expression) => {
                    compile(
                        &expression.expression,
                        expression.expression_span,
                        seen,
                        expressions,
                    )?;
                }
                TemplateNode::If(condition) => {
                    compile(
                        &condition.condition,
                        condition.condition_span,
                        seen,
                        expressions,
                    )?;
                    nodes(&condition.then_children, seen, expressions)?;
                    nodes(&condition.else_children, seen, expressions)?;
                }
                TemplateNode::For(for_node) => {
                    compile(
                        &for_node.iterable,
                        for_node.iterable_span,
                        seen,
                        expressions,
                    )?;
                    if let Some(key) = &for_node.key {
                        compile(
                            key,
                            for_node.key_span.unwrap_or_default(),
                            seen,
                            expressions,
                        )?;
                    }
                    nodes(&for_node.children, seen, expressions)?;
                }
                TemplateNode::Text(_) | TemplateNode::Slot(_) => {}
            }
        }
        Ok(())
    }

    let mut seen = HashSet::new();
    let mut expressions = Vec::new();
    if let Some(template) = template {
        nodes(&template.root, &mut seen, &mut expressions)?;
    }
    Ok(expressions)
}

/// Parse a standalone Luau script block for editor tooling.
pub fn parse_luau_script(source: &str) -> Result<crate::ScriptBlock, ParseError> {
    parse_script(source).map(|(_, script)| script)
}

/// Parse the declaration list carried by an element's `style` attribute.
pub fn parse_inline_style(source: &str) -> Result<Vec<crate::style::Declaration>, ParseError> {
    styles::parse_inline_style(source)
}

const TOP_LEVEL_BLOCKS: &[&str] = &["props", "template", "script", "style", "i18n"];

/// Parse the complete top-level grammar while retaining ranges into `source`.
///
/// The block bodies are deliberately not copied or searched with a global
/// substring operation. The scanner advances from one validated block to the
/// next, so unknown content, duplicate blocks, attributes, ordering, and
/// source ranges cannot be silently discarded.
fn parse_top_level_blocks(source: &str) -> Result<Vec<ComponentBlock>, ParseError> {
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0;

    while offset < source.len() {
        offset = skip_ascii_whitespace(source, offset);
        if offset == source.len() {
            break;
        }

        if source.as_bytes()[offset] != b'<' {
            return Err(ParseError::UnexpectedTopLevelContent {
                message: "expected a component block".into(),
                span: SourceSpan::new(offset, offset + 1),
            });
        }

        if source[offset..].starts_with("</") {
            let name = scan_tag_name(source, offset + 2).unwrap_or_default();
            return Err(ParseError::UnexpectedClose {
                tag: name,
                span: SourceSpan::new(
                    offset,
                    find_tag_end(source, offset + 2).map_or(offset + 2, |end| end + 1),
                ),
            });
        }

        let opening = parse_opening_tag(source, offset)?;
        if !TOP_LEVEL_BLOCKS.contains(&opening.name.as_str()) {
            return Err(ParseError::UnknownBlock {
                name: opening.name,
                span: SourceSpan::new(offset, opening.end),
            });
        }

        if !seen.insert(opening.name.clone()) {
            return Err(ParseError::DuplicateBlock {
                name: opening.name,
                span: SourceSpan::new(offset, opening.end),
            });
        }

        validate_block_attributes(offset, &opening)?;

        let content_start = opening.end;
        let (content_end, close_end) = find_block_close(source, content_start, &opening.name);
        let Some((content_end, close_end)) = content_end.zip(close_end) else {
            return Err(ParseError::UnclosedBlock {
                tag: opening.name,
                span: SourceSpan::new(offset, opening.end),
            });
        };

        if opening.name == "i18n" {
            return Err(ParseError::InvalidI18n {
                message: format!(
                    "inline catalogs are not supported; declare files in mesh.provides.i18n (line {})",
                    line_at(source, offset)
                ),
                span: SourceSpan::new(offset, close_end),
            });
        }

        blocks.push(ComponentBlock {
            name: opening.name,
            attributes: opening.attributes,
            span: SourceSpan::new(offset, close_end),
            open_tag: SourceSpan::new(offset, opening.end),
            content: SourceSpan::new(content_start, content_end),
            close_tag: SourceSpan::new(content_end, close_end),
        });
        offset = close_end;
    }

    if !seen.contains("template") {
        return Err(ParseError::MissingRequiredBlock {
            name: "template".into(),
            span: SourceSpan::new(source.len(), source.len()),
        });
    }

    Ok(blocks)
}

struct OpeningTag {
    name: String,
    attributes: Vec<BlockAttribute>,
    end: usize,
}

fn parse_opening_tag(source: &str, start: usize) -> Result<OpeningTag, ParseError> {
    let Some(end) = find_tag_end(source, start + 1) else {
        return Err(ParseError::MalformedTopLevelBlock {
            message: "opening tag has no closing `>`".into(),
            span: SourceSpan::new(start, source.len()),
        });
    };

    let name_start = start + 1;
    let Some(name_end) = scan_tag_name_end(source, name_start) else {
        return Err(ParseError::MalformedTopLevelBlock {
            message: "opening tag has no valid name".into(),
            span: SourceSpan::new(start, end + 1),
        });
    };
    let name = source[name_start..name_end].to_string();
    let mut cursor = name_end;
    let mut attributes = Vec::new();
    let mut attribute_names = HashSet::new();

    while cursor < end - 1 {
        cursor = skip_ascii_whitespace(source, cursor);
        if cursor >= end - 1 {
            break;
        }
        if source.as_bytes()[cursor] == b'/' {
            return Err(ParseError::InvalidBlockAttributes {
                name,
                message: "top-level blocks must use an explicit closing tag".into(),
                span: SourceSpan::new(cursor, end + 1),
            });
        }

        let attribute_start = cursor;
        let Some(attribute_end) = scan_attribute_name_end(source, cursor) else {
            return Err(ParseError::MalformedTopLevelBlock {
                message: "invalid attribute name".into(),
                span: SourceSpan::new(cursor, end + 1),
            });
        };
        let attribute_name = source[attribute_start..attribute_end].to_string();
        cursor = skip_ascii_whitespace(source, attribute_end);
        if source.as_bytes().get(cursor) != Some(&b'=') {
            return Err(ParseError::InvalidBlockAttributes {
                name,
                message: format!("attribute `{attribute_name}` must have a quoted value"),
                span: SourceSpan::new(attribute_start, attribute_end),
            });
        }
        cursor = skip_ascii_whitespace(source, cursor + 1);
        let Some(&quote) = source.as_bytes().get(cursor) else {
            return Err(ParseError::MalformedTopLevelBlock {
                message: "attribute value is missing".into(),
                span: SourceSpan::new(cursor, end + 1),
            });
        };
        if quote != b'"' && quote != b'\'' {
            return Err(ParseError::InvalidBlockAttributes {
                name,
                message: format!("attribute `{attribute_name}` must use single or double quotes"),
                span: SourceSpan::new(attribute_start, cursor + 1),
            });
        }
        let value_start = cursor + 1;
        let Some(relative_end) = source[value_start..].find(quote as char) else {
            return Err(ParseError::MalformedTopLevelBlock {
                message: format!("attribute `{attribute_name}` is unclosed"),
                span: SourceSpan::new(attribute_start, end + 1),
            });
        };
        let value_end = value_start + relative_end;
        cursor = value_end + 1;
        if !attribute_names.insert(attribute_name.clone()) {
            return Err(ParseError::InvalidBlockAttributes {
                name,
                message: format!("duplicate attribute `{attribute_name}`"),
                span: SourceSpan::new(attribute_start, cursor),
            });
        }
        attributes.push(BlockAttribute {
            name: attribute_name,
            value: source[value_start..value_end].to_string(),
            span: SourceSpan::new(attribute_start, cursor),
            value_span: SourceSpan::new(value_start, value_end),
        });
    }

    Ok(OpeningTag {
        name,
        attributes,
        end: end + 1,
    })
}

fn validate_block_attributes(start: usize, opening: &OpeningTag) -> Result<(), ParseError> {
    match opening.name.as_str() {
        "script" => {
            // Luau is the established default for an unadorned script block.
            // When `lang` is present, it is still validated explicitly so an
            // unsupported runtime language cannot be silently accepted.
            if opening.attributes.is_empty() {
                return Ok(());
            }
            let Some(attribute) = opening.attributes.first() else {
                return Err(ParseError::InvalidBlockAttributes {
                    name: opening.name.clone(),
                    message: "expected lang=\"luau\"".into(),
                    span: SourceSpan::new(start, opening.end),
                });
            };
            if opening.attributes.len() != 1 || attribute.name != "lang" {
                return Err(ParseError::InvalidBlockAttributes {
                    name: opening.name.clone(),
                    message: "only lang=\"luau\" is supported".into(),
                    span: SourceSpan::new(start, opening.end),
                });
            }
            if attribute.value != "luau" {
                return Err(ParseError::UnsupportedScriptLanguage {
                    language: attribute.value.clone(),
                    span: attribute.value_span,
                });
            }
        }
        "template" | "props" | "style" => {
            if !opening.attributes.is_empty() {
                return Err(ParseError::InvalidBlockAttributes {
                    name: opening.name.clone(),
                    message: "this block does not accept attributes".into(),
                    span: SourceSpan::new(start, opening.end),
                });
            }
        }
        "i18n" => {}
        _ => {}
    }
    Ok(())
}

fn find_block_close(source: &str, start: usize, name: &str) -> (Option<usize>, Option<usize>) {
    let mut cursor = start;
    let mut nested = 0usize;
    let mut quote = None;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut long_delimiter = None;

    while cursor < source.len() {
        if let Some(delimiter) = long_delimiter {
            if let Some(end) = find_long_bracket_end(source, cursor, delimiter) {
                cursor = end;
                long_delimiter = None;
                continue;
            }
            return (None, None);
        }
        if line_comment {
            if source.as_bytes()[cursor] == b'\n' {
                line_comment = false;
            }
            cursor = advance_char(source, cursor);
            continue;
        }
        if block_comment {
            if source[cursor..].starts_with("*/") {
                block_comment = false;
                cursor += 2;
            } else {
                cursor = advance_char(source, cursor);
            }
            continue;
        }
        if let Some(active_quote) = quote {
            let byte = source.as_bytes()[cursor];
            if byte == b'\\' {
                cursor = skip_escaped_char(source, cursor);
            } else {
                if byte == active_quote {
                    quote = None;
                }
                cursor = advance_char(source, cursor);
            }
            continue;
        }

        if source[cursor..].starts_with("<!--") {
            if let Some(end) = source[cursor + 4..].find("-->") {
                cursor += 4 + end + 3;
                continue;
            }
            return (None, None);
        }
        if source[cursor..].starts_with("/*") {
            block_comment = true;
            cursor += 2;
            continue;
        }
        if name == "script" && source[cursor..].starts_with("--") {
            if source[cursor..].starts_with("--[") {
                if let Some(delimiter) = long_bracket_delimiter(source, cursor + 2) {
                    long_delimiter = Some(delimiter);
                    cursor += 2 + delimiter.open_len;
                    continue;
                }
            }
            line_comment = true;
            cursor += 2;
            continue;
        }
        if name == "script"
            && source.as_bytes()[cursor] == b'['
            && let Some(delimiter) = long_bracket_delimiter(source, cursor)
        {
            long_delimiter = Some(delimiter);
            cursor += delimiter.open_len;
            continue;
        }

        // In template text, quote characters are ordinary text. Only treat
        // them as protected delimiters while skipping an actual markup tag.
        // This keeps apostrophes in rendered text from changing the block
        // boundary while still protecting `</template>` in an attribute.
        if name == "template" {
            if source.as_bytes()[cursor] == b'<' {
                if source[cursor..].starts_with("</") {
                    if let Some((tag_name, tag_end)) = parse_close_tag(source, cursor)
                        && tag_name == name
                    {
                        if nested == 0 {
                            return (Some(cursor), Some(tag_end));
                        }
                        nested -= 1;
                        cursor = tag_end;
                    } else {
                        cursor = advance_char(source, cursor);
                    }
                } else if let Some((tag_name, tag_end, self_closing)) =
                    parse_inner_open_tag(source, cursor)
                {
                    if tag_name == name && !self_closing {
                        nested += 1;
                    }
                    cursor = tag_end;
                } else {
                    cursor = advance_char(source, cursor);
                }
            } else {
                cursor = advance_char(source, cursor);
            }
            continue;
        }

        match source.as_bytes()[cursor] {
            b'"' | b'\'' => {
                quote = Some(source.as_bytes()[cursor]);
                cursor = advance_char(source, cursor);
            }
            b'<' if source[cursor..].starts_with("</") => {
                if let Some((tag_name, tag_end)) = parse_close_tag(source, cursor)
                    && tag_name == name
                {
                    if nested == 0 {
                        return (Some(cursor), Some(tag_end));
                    }
                    nested -= 1;
                    cursor = tag_end;
                } else {
                    cursor = advance_char(source, cursor);
                }
            }
            b'<' => {
                if let Some((tag_name, tag_end, self_closing)) =
                    parse_inner_open_tag(source, cursor)
                    && tag_name == name
                    && !self_closing
                {
                    nested += 1;
                    cursor = tag_end;
                } else {
                    cursor = advance_char(source, cursor);
                }
            }
            _ => cursor = advance_char(source, cursor),
        }
    }
    (None, None)
}

#[derive(Clone, Copy)]
struct LongBracketDelimiter {
    open_len: usize,
    close_len: usize,
}

fn long_bracket_delimiter(source: &str, start: usize) -> Option<LongBracketDelimiter> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut cursor = start + 1;
    while bytes.get(cursor) == Some(&b'=') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'[') {
        return None;
    }
    let equals = cursor - start - 1;
    Some(LongBracketDelimiter {
        open_len: cursor + 1 - start,
        close_len: equals + 2,
    })
}

fn find_long_bracket_end(
    source: &str,
    start: usize,
    delimiter: LongBracketDelimiter,
) -> Option<usize> {
    let close = format!("]{}]", "=".repeat(delimiter.close_len.saturating_sub(2)));
    source[start..]
        .find(&close)
        .map(|offset| start + offset + close.len())
}

fn parse_close_tag(source: &str, start: usize) -> Option<(String, usize)> {
    let name_start = start + 2;
    let name_end = scan_tag_name_end(source, name_start)?;
    let name = source[name_start..name_end].to_string();
    let end = find_tag_end(source, name_end)?;
    if !source[name_end..end].trim().is_empty() {
        return None;
    }
    Some((name, end + 1))
}

fn parse_inner_open_tag(source: &str, start: usize) -> Option<(String, usize, bool)> {
    let name_start = start + 1;
    let name_end = scan_tag_name_end(source, name_start)?;
    let end = find_tag_end(source, name_end)?;
    let tail = source[name_end..end].trim();
    Some((
        source[name_start..name_end].to_string(),
        end + 1,
        tail.ends_with('/'),
    ))
}

pub(super) fn find_tag_end(source: &str, start: usize) -> Option<usize> {
    let mut cursor = start;
    let mut quote = None;
    while cursor < source.len() {
        let byte = source.as_bytes()[cursor];
        if let Some(active_quote) = quote {
            if byte == b'\\' {
                cursor = (cursor + 2).min(source.len());
                continue;
            }
            if byte == active_quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn scan_tag_name(source: &str, start: usize) -> Option<String> {
    let end = scan_tag_name_end(source, start)?;
    Some(source[start..end].to_string())
}

fn scan_tag_name_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let first = *bytes.get(start)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = start + 1;
    while let Some(byte) = bytes.get(end) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':') {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

fn scan_attribute_name_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let first = *bytes.get(start)?;
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b':') {
        return None;
    }
    let mut end = start + 1;
    while let Some(byte) = bytes.get(end) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.') {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

fn skip_ascii_whitespace(source: &str, mut offset: usize) -> usize {
    while let Some(byte) = source.as_bytes().get(offset) {
        if !byte.is_ascii_whitespace() {
            break;
        }
        offset += 1;
    }
    offset
}

fn advance_char(source: &str, offset: usize) -> usize {
    source[offset..]
        .chars()
        .next()
        .map(|ch| offset + ch.len_utf8())
        .unwrap_or(offset)
}

fn skip_escaped_char(source: &str, offset: usize) -> usize {
    let after_escape = advance_char(source, offset);
    if after_escape < source.len() {
        advance_char(source, after_escape)
    } else {
        after_escape
    }
}

fn add_base(span: SourceSpan, base: usize) -> SourceSpan {
    SourceSpan::new(base + span.start, base + span.end)
}

fn line_at(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ComponentImportTarget, ScriptLang,
        style::{
            ContainerQuery, Selector, StyleValue, TransitionEasing,
            is_transition_safe_keyframe_property,
        },
        template::{AttributeValue, TemplateNode},
    };

    #[test]
    fn parse_minimal_component() {
        let source = r#"
<template>
  <text>Hello</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        assert!(file.template.is_some());
        assert!(file.script.is_none());
        assert!(file.style.is_none());
        assert!(file.props.is_none());
    }

    #[test]
    fn component_ast_nodes_retain_absolute_source_spans() {
        let source = "\n<props>\nwidth: { type: \"size\", default: \"10px\" }\n</props>\n<template>\n  <box><text>Hello</text></box>\n</template>\n<script lang=\"luau\">\nlocal count = 1\n</script>\n<style>\nbox { opacity: 1; }\n</style>\n";
        let file = parse_component(source).expect("component parses");

        let props = file.props.as_ref().expect("props block");
        assert_eq!(
            &source[props.span.start..props.span.end],
            "\nwidth: { type: \"size\", default: \"10px\" }\n"
        );
        let prop = &props.props[0];
        assert_eq!(
            &source[prop.span.start..prop.span.end],
            "width: { type: \"size\", default: \"10px\" }"
        );

        let template = file.template.as_ref().expect("template block");
        let TemplateNode::Element(box_node) = &template.root[0] else {
            panic!("expected root element");
        };
        let box_start = source.find("<box>").expect("box opening");
        let box_end = source.find("</box>").expect("box closing") + "</box>".len();
        assert_eq!(box_node.span, SourceSpan::new(box_start, box_end));
        let TemplateNode::Element(text_node) = &box_node.children[0] else {
            panic!("expected nested text element");
        };
        let text_start = source.find("<text>").expect("text opening");
        let text_end = source.find("</text>").expect("text closing") + "</text>".len();
        assert_eq!(text_node.span, SourceSpan::new(text_start, text_end));
        let TemplateNode::Text(text) = &text_node.children[0] else {
            panic!("expected text node");
        };
        let hello_start = source.find("Hello").expect("text content");
        assert_eq!(text.span, SourceSpan::new(hello_start, hello_start + 5));

        let script = file.script.as_ref().expect("script block");
        let script_start = source.find("\nlocal count").expect("script body");
        let script_end = source.find("\n</script>").expect("script close") + 1;
        assert_eq!(script.span, SourceSpan::new(script_start, script_end));
        let style = file.style.as_ref().expect("style block");
        let style_start = source.find("\nbox { opacity").expect("style body");
        let style_end = source.find("\n</style>").expect("style close") + 1;
        assert_eq!(style.span, SourceSpan::new(style_start, style_end));
    }

    #[test]
    fn parse_errors_retain_a_typed_category_with_their_source_span() {
        let source = "<template><text /></template>\n<style>oops</style>";
        let error = parse_component(source).expect_err("invalid style should be rejected");

        assert_eq!(error.category(), ParseDiagnosticCategory::Style);
        assert!(error.span().start > source.find("<style>").unwrap());
        assert!(error.span().end > error.span().start);
    }

    #[test]
    fn parser_errors_retain_component_absolute_spans() {
        let source = "\n<template>\n{#if}\n</template>\n";
        let error = parse_component(source).expect_err("invalid condition accepted");
        let span = error.span();
        let directive = source.find("{#if}").expect("directive");
        assert_eq!(span, SourceSpan::new(directive, directive + 1));

        let source = "<script lang=\"luau\">value = 1</script>";
        let error = parse_component(source).expect_err("missing template accepted");
        assert_eq!(error.span(), SourceSpan::new(source.len(), source.len()));
    }

    #[test]
    fn preserves_top_level_block_order_and_source_spans() {
        let source = "  \n<template>\n  <box />\n</template>\n\n<script lang='luau'>\nvalue = 1\n</script>\n<style>\nbox { opacity: 1; }\n</style>\n";
        let file = parse_component(source).unwrap();

        let names: Vec<_> = file
            .blocks
            .iter()
            .map(|block| block.name.as_str())
            .collect();
        assert_eq!(names, ["template", "script", "style"]);

        let script = &file.blocks[1];
        assert_eq!(script.attributes[0].name, "lang");
        assert_eq!(script.attributes[0].value, "luau");
        assert_eq!(
            &source[script.attributes[0].value_span.start..script.attributes[0].value_span.end],
            "luau"
        );
        assert_eq!(
            &source[script.span.start..script.span.end],
            "<script lang='luau'>\nvalue = 1\n</script>"
        );
        assert_eq!(
            &source[script.content.start..script.content.end],
            "\nvalue = 1\n"
        );
        assert_eq!(
            &source[script.open_tag.start..script.open_tag.end],
            "<script lang='luau'>"
        );
        assert_eq!(
            &source[script.close_tag.start..script.close_tag.end],
            "</script>"
        );
    }

    #[test]
    fn rejects_invalid_top_level_block_grammar() {
        let cases = [
            (
                "<template><box /></template><unknown></unknown>",
                "unknown block <unknown>",
            ),
            (
                "<template><box /></template><template></template>",
                "duplicate block <template>",
            ),
            (
                "<style>box { opacity: 1; }</style>",
                "missing required block <template>",
            ),
            (
                "<template data='x'><box /></template>",
                "invalid attributes on <template>",
            ),
            (
                "<template><box /></template><script lang='lua'></script>",
                "unsupported script language `lua`",
            ),
        ];

        for (source, expected) in cases {
            let error = parse_component(source).expect_err("invalid top-level grammar");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn block_closing_tags_inside_block_literals_are_not_boundaries() {
        let source = r#"
<template><text title="</template>">It's fine</text></template>
<script lang="luau">local marker = "</script>"</script>
<style>.literal::after { content: "</style>"; }</style>
"#;
        let file = parse_component(source).unwrap();

        assert_eq!(file.blocks.len(), 3);
        assert!(file.script.unwrap().source.contains("</script>"));
        assert_eq!(file.style.unwrap().rules.len(), 1);
    }

    #[test]
    fn rejects_stray_top_level_content_and_inline_i18n() {
        let error = parse_component("text\n<template><box /></template>").unwrap_err();
        assert!(error.to_string().contains("unexpected top-level content"));

        let error =
            parse_component("<template><box /></template>\n<i18n>{\"hello\": \"Hello\"}</i18n>")
                .unwrap_err();
        assert!(matches!(error, ParseError::InvalidI18n { .. }));
        assert!(error.to_string().contains("mesh.provides.i18n"));
    }

    #[test]
    fn parse_props_block_through_component() {
        use crate::{PropType, PropValue};
        let source = r#"
<props>
  track_width: { type: "size", default: "20px", label: t("var.track_width") }
  anim_ms:     { type: "duration", default: 120, min: 0, max: 600 }
</props>

<template>
  <slider class="audio-slider"/>
</template>

<style>
.audio-slider { width: prop(track_width); }
</style>
"#;
        let file = parse_component(source).unwrap();
        let props = file.props.expect("props block parsed");
        assert_eq!(props.props.len(), 2);
        assert_eq!(props.props[0].name, "track_width");
        assert_eq!(props.props[0].ty, PropType::Size);
        assert_eq!(
            props.props[0].default,
            Some(PropValue::String("20px".into()))
        );
        assert_eq!(props.props[1].ty, PropType::Duration);
        assert_eq!(props.props[1].max, Some(600.0));
    }

    #[test]
    fn rejects_inline_i18n_with_a_migration_message_and_source_line() {
        let source = "\n<template><text>Hello</text></template>\n\n<i18n>\n{}\n</i18n>\n";
        let error = parse_component(source).expect_err("inline catalogs must be rejected");
        assert!(matches!(error, ParseError::InvalidI18n { .. }));
        let message = error.to_string();
        assert!(
            message.contains("line 4"),
            "unexpected source location: {message}"
        );
        assert!(message.contains("mesh.provides.i18n"));
    }

    #[test]
    fn rejects_removed_html_compat_tags() {
        let source = r#"
<template>
  <div>Hello</div>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string().contains("unknown UI tag <div>"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_full_component() {
        let source = r#"
<template>
  <box>
    <text class="title">{ title }</text>
    <button onclick="onTap">Click me</button>
  </box>
</template>

<script lang="luau">
local title = "Hello"
function onTap()
    title = "Clicked!"
end
</script>

<style>
.title {
    color: var(--color-on-surface);
    font-size: var(--typography-size-lg);
}

button {
    background: var(--color-primary);
    padding: 8px;
}
</style>
"#;
        let file = parse_component(source).unwrap();

        let tmpl = file.template.unwrap();
        assert_eq!(tmpl.root.len(), 1);

        let script = file.script.unwrap();
        assert_eq!(script.lang, ScriptLang::Luau);
        assert!(script.source.contains("function onTap"));

        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        match &style.rules[0].declarations[0].value {
            StyleValue::Var(name) => assert_eq!(name, "--color-on-surface"),
            other => panic!("expected var, got {other:?}"),
        }
    }

    #[test]
    fn parse_expression_interpolation() {
        let source = r#"
<template>
  <text>Time: { formatTime(time) }</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "text");
                assert!(!el.children.is_empty());
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn template_expressions_are_deduplicated_compiled_artifacts() {
        let file = parse_component(
            r#"
<template>
  <text>{count > 0}</text>
  <text>{ count > 0 }</text>
</template>
"#,
        )
        .unwrap();

        assert_eq!(file.template_expressions.len(), 1);
        let cached = crate::compile_expression("count > 0").unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &file.template_expressions[0],
            &cached
        ));
    }

    #[test]
    fn rejects_malformed_interpolation_with_source_location() {
        for (body, expected) in [
            ("{name", "unterminated interpolation"),
            ("{}", "empty interpolation"),
            ("{name + }", "malformed Luau interpolation"),
        ] {
            let source = format!("<template>\n  <text>{body}</text>\n</template>");
            let error = parse_component(&source).expect_err("malformed interpolation accepted");
            let message = error.to_string();
            assert!(message.contains(expected), "{message}");
            assert!(message.contains("line 2"), "{message}");
            assert!(message.contains("column "), "{message}");
        }
    }

    #[test]
    fn interpolation_spans_cover_braces_and_expression_body() {
        let source = "<template>\n  <text>{t(\"}\")}</text>\n</template>";
        let file = parse_component(source).expect("quoted brace expression parses");
        let TemplateNode::Element(text) = &file.template.unwrap().root[0] else {
            panic!("expected text element");
        };
        let [TemplateNode::Expr(expression)] = text.children.as_slice() else {
            panic!("expected one interpolation");
        };
        let start = source.find('{').expect("opening brace");
        let end = source.find("}</text>").expect("closing brace") + 1;
        assert_eq!(expression.span, SourceSpan::new(start, end));
        assert_eq!(
            &source[expression.expression_span.start..expression.expression_span.end],
            r#"t("}")"#
        );
    }

    #[test]
    fn control_flow_is_lexed_and_spanned_before_markup_lowering() {
        let source = r#"<template>
{#if show}
  {#for item in items key={item.id}}<text>{item.name}</text>{/for}
{:else if fallback}
  <text>fallback</text>
{:else}
  <text>hidden</text>
{/if}
</template>"#;
        let file = parse_component(source).expect("control-flow parses");
        let TemplateNode::If(if_node) = &file.template.unwrap().root[0] else {
            panic!("expected if node");
        };
        let if_start = source.find("{#if").expect("if opening");
        let if_end = source.find("{/if}").expect("if closing") + "{/if}".len();
        assert_eq!(if_node.span, SourceSpan::new(if_start, if_end));
        assert_eq!(
            &source[if_node.condition_span.start..if_node.condition_span.end],
            "show"
        );
        let TemplateNode::For(for_node) = &if_node.then_children[0] else {
            panic!("expected for node");
        };
        let for_start = source.find("{#for").expect("for opening");
        let for_end = source.find("{/for}").expect("for closing") + "{/for}".len();
        assert_eq!(for_node.span, SourceSpan::new(for_start, for_end));
        assert_eq!(
            &source[for_node.iterable_span.start..for_node.iterable_span.end],
            "items"
        );
        let key_span = for_node.key_span.expect("key span");
        assert_eq!(&source[key_span.start..key_span.end], "item.id");
    }

    #[test]
    fn rejects_mismatched_control_flow_braces() {
        for body in ["{/if}", "{#if show}{/for}", "{#if show}{:else}{:else}{/if}"] {
            let source = format!("<template>{body}</template>");
            let error = parse_component(&source).expect_err("malformed control flow accepted");
            assert!(
                error.to_string().contains("directive")
                    || error.to_string().contains("unexpected")
                    || error.to_string().contains("closes"),
                "{error}"
            );
        }
    }

    #[test]
    fn attribute_interpolations_retain_their_brace_span() {
        let source =
            r#"<template><button title={tooltip} onclick="{activate}">Open</button></template>"#;
        let file = parse_component(source).expect("attribute interpolations parse");
        let TemplateNode::Element(button) = &file.template.unwrap().root[0] else {
            panic!("expected button");
        };
        let title = &button.attributes[0];
        let title_start = source.find("{tooltip}").expect("title expression");
        assert_eq!(
            title.span,
            Some(SourceSpan::new(title_start, title_start + 9))
        );
        let onclick = &button.attributes[1];
        let onclick_start = source.find("{activate}").expect("handler expression");
        assert_eq!(
            onclick.span,
            Some(SourceSpan::new(onclick_start, onclick_start + 10))
        );
    }

    #[test]
    fn classifies_standalone_prop_reference() {
        let source = r#"
<props>
track_width: { type: "size", default: "20px" }
gap: { type: "size", default: "4px" }
</props>
<template><box /></template>
<style>
.mixer {
    width: prop(track_width);
    height: var(--track-height);
    padding: calc(prop(gap) * 2);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let decls = &file.style.unwrap().rules[0].declarations;
        // Standalone prop(name) -> StyleValue::Prop.
        assert!(matches!(&decls[0].value, StyleValue::Prop(name) if name == "track_width"));
        // var() unaffected.
        assert!(matches!(&decls[1].value, StyleValue::Var(name) if name == "--track-height"));
        // prop() embedded in calc() stays Literal for later substitution.
        assert!(
            matches!(&decls[2].value, StyleValue::Literal(value) if value.contains("prop(gap)"))
        );
    }

    #[test]
    fn rejects_undefined_and_empty_style_props() {
        for style in [
            ".root { width: prop(missing); }",
            ".root { width: prop(); }",
        ] {
            let source = format!("<template><box /></template><style>{style}</style>");
            let error = parse_component(&source).expect_err("invalid style prop accepted");
            assert!(matches!(error, ParseError::InvalidSemantics { .. }));
            assert!(error.to_string().contains("prop"));
        }
    }

    #[test]
    fn rejects_prop_type_when_css_domain_does_not_match() {
        let source = r##"
<props>
accent: { type: "color", default: "#fff" }
</props>
<template><box /></template>
<style>
.root { width: prop(accent); }
</style>
"##;
        let error = parse_component(source).expect_err("wrong CSS prop domain accepted");
        assert!(matches!(error, ParseError::InvalidSemantics { .. }));
        assert!(error.to_string().contains("requires a length value"));
    }

    #[test]
    fn accepts_embedded_props_that_match_css_domains() {
        let source = r#"
<props>
gap: { type: "size", default: "4px" }
anim_ms: { type: "duration", default: 120 }
</props>
<template><box /></template>
<style>
.root {
  width: calc(prop(gap) * 2);
  transition-duration: prop(anim_ms);
  content: "prop(not_a_component_prop)";
}
</style>
"#;
        parse_component(source).expect("matching CSS prop domains rejected");
    }

    #[test]
    fn rejects_prop_reference_in_keyframes() {
        let source = r#"
<template><box /></template>
<style>
@keyframes grow {
    0% { width: prop(track_width); }
    100% { opacity: 1; }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(err.contains("cannot use var() references"), "{err}");
    }

    #[test]
    fn parse_style_tokens_and_literals() {
        let source = r#"
<template><box /></template>
<style>
box {
    gap: 8px;
    padding: var(--spacing-md);
    background: var(--bg);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        assert!(matches!(&decls[0].value, StyleValue::Literal(v) if v == "8px"));
        assert!(matches!(&decls[1].value, StyleValue::Var(v) if v == "--spacing-md"));
        assert!(matches!(&decls[2].value, StyleValue::Var(v) if v == "--bg"));
        assert!(style.rules[0].container_query.is_none());
        assert!(style.keyframes.is_empty());
    }

    #[test]
    fn parse_grouped_selectors_into_multiple_rules() {
        let source = r#"
<template><box /></template>
<style>
.panel, #main {
    color: #fff;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        assert!(matches!(&style.rules[0].selector, Selector::Class(name) if name == "panel"));
        assert!(matches!(&style.rules[1].selector, Selector::Id(name) if name == "main"));
    }

    #[test]
    fn parse_container_query_rules() {
        let source = r#"
<template><box /></template>
<style>
@container (max-width: 640px) {
    .sidebar {
        width: 100%;
        overflow-y: auto;
    }
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 1);
        let rule = &style.rules[0];
        assert_eq!(
            rule.container_query,
            Some(ContainerQuery {
                max_width: Some(640.0),
                ..Default::default()
            })
        );
        assert!(matches!(&rule.selector, Selector::Class(name) if name == "sidebar"));
    }

    #[test]
    fn unsupported_media_rule_reports_at_rule_name() {
        let source = r#"
<template><box /></template>
<style>
@media (min-width: 640px) {
    .panel {
        color: #fff;
    }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(err.contains("unsupported at-rule '@media'"), "{err}");
    }

    #[test]
    fn keyframe_property_helper_accepts_transition_safe_properties() {
        for property in [
            "opacity",
            "transform",
            "background-color",
            "border-color",
            "border-radius",
            "padding",
            "font-size",
            "inset",
            "filter",
            "backdrop-filter",
            "box-shadow",
        ] {
            assert!(is_transition_safe_keyframe_property(property), "{property}");
        }
    }

    #[test]
    fn keyframe_property_helper_rejects_unsupported_properties() {
        for property in [
            "grid-template-columns",
            "display",
            "position",
            "container-type",
        ] {
            assert!(
                !is_transition_safe_keyframe_property(property),
                "{property}"
            );
        }
    }

    #[test]
    fn parse_percentage_keyframes() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { opacity: 0; }
    50% { opacity: 0.5; }
    100% { opacity: 1; }
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.keyframes.len(), 1);
        assert_eq!(style.keyframes[0].name, "pulse");
        assert_eq!(style.keyframes[0].stops.len(), 3);
        assert_eq!(style.keyframes[0].stops[0].offset, 0.0);
        assert_eq!(style.keyframes[0].stops[1].offset, 0.5);
        assert_eq!(style.keyframes[0].stops[2].offset, 1.0);
    }

    #[test]
    fn parse_validated_per_keyframe_easing() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { opacity: 0; animation-timing-function: ease-in; }
    50% { opacity: 0.5; animation-timing-function: steps(4, jump-start); }
    100% { opacity: 1; }
}
</style>
"#;
        let file = parse_component(source).expect("per-keyframe easing parses");
        let stops = &file.style.expect("style").keyframes[0].stops;

        assert_eq!(stops[0].easing, Some(TransitionEasing::EaseIn));
        assert_eq!(
            stops[1].easing,
            Some(TransitionEasing::Steps(
                4,
                crate::style::StepPosition::JumpStart
            ))
        );
        assert_eq!(stops[2].easing, None);
        assert!(
            stops[0]
                .declarations
                .iter()
                .all(|declaration| declaration.property != "animation-timing-function")
        );
    }

    #[test]
    fn reject_from_keyframe_alias() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    from { opacity: 0; }
    100% { opacity: 1; }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(
            err.contains("from/to keyframe aliases are not supported"),
            "{err}"
        );
    }

    #[test]
    fn reject_to_keyframe_alias() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { opacity: 0; }
    to { opacity: 1; }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(
            err.contains("from/to keyframe aliases are not supported"),
            "{err}"
        );
    }

    #[test]
    fn parse_filter_and_shadow_keyframes() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { filter: blur(4px); }
    50% { box-shadow: 0 2px 8px #00000080; }
    75% { backdrop-filter: blur(2px); }
    100% { opacity: 1; }
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let properties: Vec<_> = style.keyframes[0]
            .stops
            .iter()
            .flat_map(|stop| stop.declarations.iter())
            .map(|declaration| declaration.property.as_str())
            .collect();

        assert!(properties.contains(&"filter"));
        assert!(properties.contains(&"box-shadow"));
        assert!(properties.contains(&"backdrop-filter"));
        assert!(properties.contains(&"opacity"));
    }

    #[test]
    fn reject_unsupported_keyframe_property() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { grid-template-columns: 1fr 1fr; }
    100% { opacity: 1; }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(
            err.contains("unsupported keyframe property 'grid-template-columns'"),
            "{err}"
        );
    }

    #[test]
    fn reject_non_runnable_keyframes() {
        let source = r#"
<template><box /></template>
<style>
@keyframes pulse {
    0% { }
    100% { }
}
</style>
"#;
        let err = parse_component(source).unwrap_err().to_string();
        assert!(
            err.contains("keyframes 'pulse' has no supported animatable properties"),
            "{err}"
        );
    }

    #[test]
    fn parse_self_closing_element() {
        let source = r#"
<template>
  <icon name="battery" size="24"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "icon");
                assert_eq!(el.children.len(), 0);
                assert_eq!(el.attributes.len(), 2);
            }
            _ => panic!("expected self-closing element"),
        }
    }

    #[test]
    fn parse_binding_and_event_attributes() {
        let source = r#"
<template>
  <input value="{volume}" onchange="onVolumeChange"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::Binding(v) if v == "volume")
                );
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::EventHandler(v) if v == "onVolumeChange")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_unquoted_brace_event_attribute() {
        let source = r#"
<template>
  <button onclick={onTap}>Click</button>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::EventHandler(v) if v == "onTap")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn script_source_passed_through_unchanged() {
        let source = r#"
<template>
  <text>{title}</text>
</template>

<script lang="luau">
mesh.state.set("title", "Hello")
mesh.state.set("count", 0)

function onTap()
    local tmp = count + 1
    count = tmp
end
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(
            script
                .source
                .contains("mesh.state.set(\"title\", \"Hello\")")
        );
        assert!(script.source.contains("local tmp = count + 1"));
    }

    #[test]
    fn local_declarations_preserved_verbatim() {
        let source = r#"
<template><box /></template>
<script lang="luau">
local handler = function() end
local audio = require("mesh.audio")
mesh.service.bind("audio.muted", "audio_muted")
mesh.service.on("audio", "sync_audio_state")
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(script.source.contains("local handler = function()"));
        assert!(
            script
                .source
                .contains("local audio = require(\"mesh.audio\")")
        );
        assert!(
            script
                .source
                .contains("mesh.service.bind(\"audio.muted\", \"audio_muted\")")
        );
        assert!(
            script
                .source
                .contains("mesh.service.on(\"audio\", \"sync_audio_state\")")
        );
    }

    #[test]
    fn parse_two_way_binding() {
        let source = r#"
<template>
  <input type="text" bind:value="searchQuery"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(matches!(&el.attributes[0].value, AttributeValue::Static(_)));
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::TwoWayBinding(v) if v == "searchQuery")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_semantic_input_tags() {
        let source = r#"
<template>
  <panel>
    <text-input value="{name}"/>
    <password-input value="{secret}"/>
    <search-input value="{query}"/>
    <number-input value="{count}"/>
    <email-input value="{email}"/>
    <url-input value="{website}"/>
  </panel>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let TemplateNode::Element(root) = &tmpl.root[0] else {
            panic!("expected root element");
        };
        let tags: Vec<_> = root
            .children
            .iter()
            .map(|child| match child {
                TemplateNode::Element(el) => el.tag.as_str(),
                _ => panic!("expected input element"),
            })
            .collect();
        assert_eq!(
            tags,
            [
                "text-input",
                "password-input",
                "search-input",
                "number-input",
                "email-input",
                "url-input",
            ]
        );
    }

    #[test]
    fn rejects_uppercase_builtin_tags() {
        let source = r#"
<template>
  <Text>Not a builtin primitive</Text>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("built-in UI tag <Text> must be lowercase; use <text> instead"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_pascal_case_custom_components() {
        let source = r#"
<template>
  <BatteryWidget percent="{percent}"/>
</template>
<script lang="luau">
import BatteryWidget from "./components/battery-widget.mesh"
</script>
"#;
        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 1);
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Component(component) => assert_eq!(component.name, "BatteryWidget"),
            other => panic!("expected component ref, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unimported_pascal_case_custom_components() {
        let source = r#"
<template>
  <BatteryWidget percent="{percent}"/>
</template>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("component <BatteryWidget> is not imported"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_and_strips_explicit_imports() {
        let source = r#"
<template>
  <BatteryWidget />
  <VolumeBar />
</template>
<script lang="luau">
import BatteryWidget from "./components/battery-widget.mesh"
import VolumeBar from "@mesh/volume-bar"
import audio from "mesh.audio@>=1.0"
mesh.state.set("ready", true)
</script>
"#;
        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 3);
        assert!(matches!(
            file.imports[0].target,
            ComponentImportTarget::ComponentLocal(_)
        ));
        assert!(matches!(
            file.imports[1].target,
            ComponentImportTarget::ComponentModule(_)
        ));
        assert!(matches!(
            file.imports[2].target,
            ComponentImportTarget::InterfaceApi { .. }
        ));
        let first_import = &file.imports[0];
        let first_start = source.find("import BatteryWidget").expect("first import");
        let first_end = source[first_start..]
            .find('\n')
            .map(|offset| first_start + offset)
            .expect("import line end");
        assert_eq!(
            &source[first_import.span.start..first_import.span.end],
            &source[first_start..first_end]
        );
        assert_eq!(
            &source[first_import.alias_span.start..first_import.alias_span.end],
            "BatteryWidget"
        );
        let script = file.script.unwrap();
        assert!(!script.source.contains("import BatteryWidget"));
        assert_eq!(script.source.lines().count(), 5);
    }

    #[test]
    fn parses_luau_require_imports_without_stripping_source() {
        let source = r#"
<template>
  <BatteryWidget />
  <VolumeBar />
</template>
<script lang="luau">
local BatteryWidget = require("./components/battery-widget.mesh")
local VolumeBar = require("@mesh/volume-bar")
local audio = require("mesh.audio@>=1.0")
</script>
"#;
        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 3);
        assert!(matches!(
            file.imports[0].target,
            ComponentImportTarget::ComponentLocal(_)
        ));
        assert!(matches!(
            file.imports[1].target,
            ComponentImportTarget::ComponentModule(_)
        ));
        assert!(matches!(
            file.imports[2].target,
            ComponentImportTarget::InterfaceApi { .. }
        ));
        let script = file.script.unwrap();
        assert!(
            script
                .source
                .contains("local BatteryWidget = require(\"./components/battery-widget.mesh\")")
        );
    }

    #[test]
    fn derives_script_metadata_from_luau_ast() {
        let source = r#"
<template><box /></template>
<script lang="luau">
-- function Fake() end
local docs = [=[import Fake from "./fake.mesh" mesh.state.set("fake", true) function Fake() end]=]
import
  BatteryWidget
from
  "./battery.mesh"
local audio = require(
  "mesh.audio@>=1.0"
)
mesh.state.set(
  "ready",
  true
)
mesh.service.bind(
  "battery.level",
  "battery_level"
)
function
  refresh()
end
</script>
"#;

        let file = parse_component(source).unwrap();
        assert_eq!(file.imports.len(), 2);
        let script = file.script.unwrap();
        assert!(script.metadata.state_vars.contains(&"ready".to_string()));
        assert!(!script.metadata.state_vars.contains(&"fake".to_string()));
        assert!(script.metadata.functions.contains(&"refresh".to_string()));
        assert!(!script.metadata.functions.contains(&"Fake".to_string()));
        assert_eq!(
            script.metadata.service_bindings,
            vec![("battery.level".into(), "battery_level".into())]
        );
        assert_eq!(
            script.metadata.interface_proxies.get("audio"),
            Some(&"mesh.audio".to_string())
        );
        assert!(script.metadata.interface_event_subscriptions.is_empty());
        assert!(!script.source.contains("\nimport\n"));
    }

    #[test]
    fn derives_interface_event_subscriptions_from_luau_ast() {
        let source = r#"
<template><box /></template>
<script lang="luau">
local audio = require(
  "mesh.audio"
)
local power = import(
  "mesh.power"
)
-- audio.FakeChanged:on(function() end)
local documentation = "power.FakeChanged:on(function() end)"
audio.events.DeviceChanged:subscribe(function(_event) end)
power.BatteryChanged:on(function(_event) end)
audio.VolumeChanged:on(function(_event) end)
</script>
"#;

        let file = parse_component(source).unwrap();
        assert_eq!(
            file.script.unwrap().metadata.interface_event_subscriptions,
            vec![
                ("mesh.audio".into(), "DeviceChanged".into()),
                ("mesh.power".into(), "BatteryChanged".into()),
                ("mesh.audio".into(), "VolumeChanged".into()),
            ]
        );
    }

    #[test]
    fn referenced_identifiers_use_luau_syntax() {
        assert_eq!(
            super::referenced_identifiers(
                r#"value .. object.field .. [=[not_an_identifier]=] -- ignored"#
            ),
            vec!["value", "object"]
        );
    }

    #[test]
    fn parses_component_bind_this_attribute() {
        let source = r#"
<template>
  <AudioSlider bind:this="{audio_slider}" />
</template>
<script lang="luau">
local AudioSlider = require("./audio-slider.mesh")
</script>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Component(component) => {
                assert_eq!(component.name, "AudioSlider");
                assert!(matches!(
                    &component.props[0].value,
                    AttributeValue::InstanceBinding(value) if value == "audio_slider"
                ));
            }
            other => panic!("expected component ref, got {other:?}"),
        }
    }

    #[test]
    fn rejects_api_import_used_as_component_tag() {
        let source = r#"
<template>
  <Audio />
</template>
<script lang="luau">
import Audio from "mesh.audio"
</script>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("component <Audio> refers to interface import"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_import_aliases() {
        let source = r#"
<template><box /></template>
<script lang="luau">
import Thing from "./components/one.mesh"
import Thing from "./components/two.mesh"
</script>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string().contains("duplicate import alias `Thing`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_alias_between_import_and_require() {
        let source = r#"
<template><box /></template>
<script lang="luau">
import Thing from "./components/one.mesh"
local Thing = require("./components/two.mesh")
</script>
"#;
        let err = parse_component(source).unwrap_err();
        assert!(
            err.to_string().contains("duplicate import alias `Thing`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_named_slot() {
        let source = r#"
<template>
  <box>
    <slot extension-point="mesh.settings.page"/>
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => match &el.children[0] {
                TemplateNode::Slot(slot) => {
                    assert_eq!(slot.extension_point.as_deref(), Some("mesh.settings.page"));
                    assert_eq!(slot.name, None);
                    assert!(!slot.customizable);
                }
                other => panic!("expected slot node, got {other:?}"),
            },
            other => panic!("expected element node, got {other:?}"),
        }
    }

    #[test]
    fn parses_customizable_slot_and_rejects_dynamic_identity() {
        let file = parse_component(
            r#"<template><slot name="start" extension-point="mesh.navigation.item" mode="customizable"/></template>"#,
        )
        .unwrap();
        let TemplateNode::Slot(slot) = &file.template.unwrap().root[0] else {
            panic!("expected slot")
        };
        assert_eq!(slot.name.as_deref(), Some("start"));
        assert!(slot.customizable);

        let error = parse_component(
            r#"<template><slot name={slot_name} extension-point="mesh.navigation.item" mode="customizable"/></template>"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("must be static"));
    }

    #[test]
    fn padding_property_names_from_lightningcss() {
        // Verify which property names lightningcss emits for the padding variants
        // we use in .mesh components so apply_declaration handles them all.
        let source = r#"
<template><box /></template>
<style>
.box {
    padding: 8px;
    padding-inline: 16px;
    padding-block: 12px;
    padding-top: 4px;
    padding-right: 5px;
    padding-bottom: 6px;
    padding-left: 7px;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        let props: Vec<&str> = decls.iter().map(|d| d.property.as_str()).collect();
        // Confirm every declaration landed under a name our style resolver knows.
        let known = [
            "padding",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "padding-inline",
            "padding-block",
            "padding-inline-start",
            "padding-inline-end",
            "padding-block-start",
            "padding-block-end",
        ];
        for p in &props {
            assert!(known.contains(p), "unrecognised padding property: {p}");
        }
        // Spot-check that the shorthand emitted individual sides (lightningcss expands it).
        eprintln!("padding properties emitted by lightningcss: {props:?}");
    }

    #[test]
    fn parse_for_loop() {
        let source = r#"
<template>
  <box>
    {#for item in items}
      <text>{item.name}</text>
    {/for}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "box");
                assert_eq!(el.children.len(), 1);
                match &el.children[0] {
                    TemplateNode::For(f) => {
                        assert_eq!(f.item_name, "item");
                        assert_eq!(f.iterable, "items");
                        assert_eq!(f.key, None);
                        assert_eq!(f.children.len(), 1);
                    }
                    other => panic!("expected ForNode, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parse_keyed_for_loop() {
        let file = parse_component(
            r#"<template>{#for item in items key={item.id}}<text>{item.name}</text>{/for}</template>"#,
        )
        .unwrap();
        let template = file.template.unwrap();
        let TemplateNode::For(for_node) = &template.root[0] else {
            panic!("expected ForNode");
        };
        assert_eq!(for_node.item_name, "item");
        assert_eq!(for_node.iterable, "items");
        assert_eq!(for_node.key.as_deref(), Some("item.id"));
    }

    #[test]
    fn parse_if_else() {
        let source = r#"
<template>
  <box>
    {#if show}
      <text>visible</text>
    {:else}
      <text>hidden</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.children.len(), 1);
                match &el.children[0] {
                    TemplateNode::If(n) => {
                        assert_eq!(n.condition, "show");
                        assert_eq!(n.then_children.len(), 1);
                        assert_eq!(n.else_children.len(), 1);
                    }
                    other => panic!("expected IfNode, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parse_if_elif_else() {
        let source = r#"
<template>
  <box>
    {#if a}
      <text>a</text>
    {:else if b}
      <text>b</text>
    {:else}
      <text>c</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let root = match &tmpl.root[0] {
            TemplateNode::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        // Outer if: condition "a"
        let outer = match &root.children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected IfNode, got {other:?}"),
        };
        assert_eq!(outer.condition, "a");
        // Else branch is another IfNode (the elif chain)
        assert_eq!(outer.else_children.len(), 1);
        let inner = match &outer.else_children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected nested IfNode, got {other:?}"),
        };
        assert_eq!(inner.condition, "b");
        assert_eq!(inner.else_children.len(), 1); // the final else
    }

    #[test]
    fn parse_for_inside_if() {
        let source = r#"
<template>
  <box>
    {#if items and #items > 0}
      {#for item in items}
        <text>{item.name}</text>
      {/for}
    {:else}
      <text>empty</text>
    {/if}
  </box>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        let root = match &tmpl.root[0] {
            TemplateNode::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        let if_node = match &root.children[0] {
            TemplateNode::If(n) => n,
            other => panic!("expected IfNode, got {other:?}"),
        };
        assert_eq!(if_node.condition, "items and #items > 0");
        assert_eq!(if_node.then_children.len(), 1);
        match &if_node.then_children[0] {
            TemplateNode::For(f) => {
                assert_eq!(f.item_name, "item");
                assert_eq!(f.iterable, "items");
            }
            other => panic!("expected ForNode, got {other:?}"),
        }
        assert_eq!(if_node.else_children.len(), 1);
    }
}
