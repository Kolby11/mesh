use crate::{ComponentFile, SourceSpan};
use cssparser::{Delimiter, Parser, ParserInput, Token};
use lightningcss::{traits::Parse, values::color::CssColor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentColorLiteral {
    pub value: String,
    pub span: SourceSpan,
}

/// Fixed colours in component style declarations, with source byte spans.
/// Token/prop expressions, transparent/currentColor, and declarations with a
/// trailing `/* mesh-allow-literal */` comment are exempt. Module independence
/// is applied by the caller, which owns the manifest context.
pub fn component_color_literals(
    component: &ComponentFile,
    source: &str,
) -> Vec<ComponentColorLiteral> {
    let Some(style) = &component.style else {
        return Vec::new();
    };
    let Some(css) = source.get(style.span.start..style.span.end) else {
        return Vec::new();
    };
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut literals = Vec::new();
    scan_rules(&mut parser, css, false, 0, &mut literals);
    for literal in &mut literals {
        literal.span.start += style.span.start;
        literal.span.end += style.span.start;
    }
    literals
}

fn color_property(name: &str) -> bool {
    name.starts_with("--")
        || name.ends_with("color")
        || matches!(
            name,
            "background"
                | "background-image"
                | "border"
                | "border-top"
                | "border-right"
                | "border-bottom"
                | "border-left"
                | "border-inline"
                | "border-block"
                | "border-image"
                | "border-image-source"
                | "text-decoration"
                | "column-rule"
                | "filter"
                | "backdrop-filter"
                | "outline"
                | "box-shadow"
                | "text-shadow"
                | "fill"
                | "stroke"
        )
}

fn scan_rules(
    parser: &mut Parser<'_, '_>,
    source: &str,
    declarations: bool,
    depth: usize,
    literals: &mut Vec<ComponentColorLiteral>,
) {
    if depth >= 128 {
        return;
    }
    let mut statement_start = true;
    while let Ok(token) = parser.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::WhiteSpace(_) | Token::Comment(_) => continue,
            Token::Semicolon => statement_start = true,
            Token::CurlyBracketBlock => {
                let _: Result<(), cssparser::ParseError<'_, ()>> =
                    parser.parse_nested_block(|inner| {
                        scan_rules(inner, source, true, depth + 1, literals);
                        Ok(())
                    });
                statement_start = true;
            }
            Token::Ident(name) if declarations && statement_start => {
                statement_start = false;
                if parser.try_parse(|p| p.expect_colon()).is_err() {
                    continue;
                }
                let first = literals.len();
                let value_start = parser.position().byte_index();
                let _: Result<(), cssparser::ParseError<'_, ()>> =
                    parser.parse_until_before(Delimiter::Semicolon, |inner| {
                        if color_property(&name.to_ascii_lowercase()) {
                            scan_value(inner, source, depth + 1, literals);
                        } else {
                            while inner.next().is_ok() {}
                        }
                        Ok(())
                    });
                let value_end = parser.position().byte_index();
                let allowed_in_value = source[value_start..value_end]
                    .trim_end()
                    .ends_with("/* mesh-allow-literal */");
                let tail = &source[value_end..];
                let tail = tail.strip_prefix(';').unwrap_or(tail);
                let same_line = tail.split(['\n', '\r']).next().unwrap_or("");
                if allowed_in_value
                    || same_line
                        .trim_start()
                        .starts_with("/* mesh-allow-literal */")
                {
                    literals.truncate(first);
                }
            }
            _ => statement_start = false,
        }
    }
}

fn scan_value(
    parser: &mut Parser<'_, '_>,
    source: &str,
    depth: usize,
    literals: &mut Vec<ComponentColorLiteral>,
) {
    if depth >= 128 {
        return;
    }
    loop {
        let start = parser.position().byte_index();
        let Ok(token) = parser.next_including_whitespace_and_comments().cloned() else {
            break;
        };
        let mut is_color = false;
        match token {
            Token::Ident(name) => {
                is_color = !name.eq_ignore_ascii_case("transparent")
                    && !name.eq_ignore_ascii_case("currentcolor")
                    && CssColor::parse_string(&name).is_ok();
            }
            Token::Hash(_) | Token::IDHash(_) => {
                is_color =
                    CssColor::parse_string(&source[start..parser.position().byte_index()]).is_ok();
            }
            Token::Function(name) => {
                let name = name.to_ascii_lowercase();
                if matches!(name.as_str(), "var" | "prop" | "url") {
                    continue;
                }
                is_color = matches!(
                    name.as_str(),
                    "rgb"
                        | "rgba"
                        | "hsl"
                        | "hsla"
                        | "hwb"
                        | "lab"
                        | "lch"
                        | "oklab"
                        | "oklch"
                        | "color"
                        | "device-cmyk"
                );
                let _: Result<(), cssparser::ParseError<'_, ()>> =
                    parser.parse_nested_block(|inner| {
                        if is_color {
                            while inner.next().is_ok() {}
                        } else {
                            scan_value(inner, source, depth + 1, literals);
                        }
                        Ok(())
                    });
            }
            _ => {}
        }
        if is_color {
            let end = parser.position().byte_index();
            literals.push(ComponentColorLiteral {
                value: source[start..end].to_string(),
                span: SourceSpan::new(start, end),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_colours_in_declarations_gradients_shadows_and_nested_rules() {
        let source = r##"<template><text>red #fff</text></template>
<style>
/* red #fff */
.red { color: #1234; background: linear-gradient(red, rgb(1 2 3 / 40%));
    box-shadow: 0 2px 4px hsl(10 20% 30%); --accent: rebeccapurple;
    content: "red #fff"; font-family: red; background-image: url("red.png"); }
@container (min-width: 10px) { .box { border: 1px solid blue; } }
@keyframes tint { from { color: #abcdef; } to { opacity: 1; } }
</style>"##;
        let component = crate::parse_component(source).unwrap();
        let literals = component_color_literals(&component, source);
        assert_eq!(
            literals
                .iter()
                .map(|l| l.value.as_str())
                .collect::<Vec<_>>(),
            [
                "#1234",
                "red",
                "rgb(1 2 3 / 40%)",
                "hsl(10 20% 30%)",
                "rebeccapurple",
                "blue",
                "#abcdef"
            ]
        );
        for literal in literals {
            assert_eq!(&source[literal.span.start..literal.span.end], literal.value);
        }
    }

    #[test]
    fn exemptions_are_scoped_to_the_expression_or_trailing_declaration() {
        let source = r##"<props>{ accent = { type = "color", default = "var(--color-primary)" } }</props><template><box/></template><style>
.box { color: var(--color-primary, #fff); background: prop(accent);
    border-color: transparent; outline-color: currentColor;
    --brand: #abc; /* mesh-allow-literal */ color: red;
    background: blue;
    --fixed: #abc /* mesh-allow-literal */;
    /* mesh-allow-literal */
    border: 1px solid green; }
</style>"##;
        let component = crate::parse_component(source).unwrap();
        assert_eq!(
            component_color_literals(&component, source)
                .iter()
                .map(|l| l.value.as_str())
                .collect::<Vec<_>>(),
            ["red", "blue", "green"]
        );
    }
}
