//! Public module styling targets and the user stylesheet layer.

use crate::css::{self, Declaration, Rule, Selector};
use cssparser::{Delimiter, Parser, ParserInput};

#[derive(Debug, Clone)]
pub struct PartRule {
    pub module: String,
    pub part: String,
    pub state: Selector,
    pub declarations: Vec<Declaration>,
}

pub fn valid_part_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

/// Parse user CSS. Only scoped module `::part(name)` rules are accepted;
/// declarations use the platform's ordinary CSS parser and value semantics.
pub fn parse_user_stylesheet(source: &str) -> Result<Vec<PartRule>, String> {
    if source.len() > 256 * 1024 {
        return Err("user stylesheet exceeds 256 KiB".into());
    }
    css::validate_source_structure(source, 0).map_err(|err| err.to_string())?;
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut rules = Vec::new();
    while !parser.is_exhausted() {
        parser.skip_whitespace();
        let start = parser.position();
        // Module ids use the manifest vocabulary, including numeric scopes;
        // they are not CSS at-keyword identifiers. Tokenize the header as a
        // prelude and delegate its declaration block to the shared CSS parser.
        let target = parser
            .parse_until_before(Delimiter::CurlyBracketBlock, |input| {
                while input.next_including_whitespace_and_comments().is_ok() {}
                Ok::<_, cssparser::ParseError<'_, ()>>(input.slice_from(start).trim().to_string())
            })
            .map_err(|err| format!("invalid public style target: {err:?}"))?;
        parser
            .expect_curly_bracket_block()
            .map_err(|_| format!("missing declarations for '{target}'"))?;
        let block = parser
            .parse_nested_block(|input| {
                let start = input.position();
                while input.next_including_whitespace_and_comments().is_ok() {}
                Ok::<_, cssparser::ParseError<'_, ()>>(input.slice_from(start).to_string())
            })
            .map_err(|err| format!("invalid declarations for '{target}': {err:?}"))?;
        let (module, tail) = target.split_once("::part(").ok_or_else(|| {
            format!("user styles must target @scope/module::part(name), found '{target}'")
        })?;
        let (part, suffix) = tail
            .split_once(')')
            .ok_or_else(|| format!("missing ')' in '{target}'"))?;
        let Some((scope, name)) = module.strip_prefix('@').and_then(|s| s.split_once('/')) else {
            return Err(format!("expected a scoped module id in '{target}'"));
        };
        let valid_module_segment = |value: &str| {
            !value.is_empty()
                && value != "."
                && value != ".."
                && value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        };
        if !valid_module_segment(scope) || !valid_module_segment(name) || !valid_part_name(part) {
            return Err(format!("invalid module or part name in '{target}'"));
        }
        let state = if suffix.trim().is_empty() {
            Selector::Universal
        } else {
            let mut states = Vec::new();
            for state in suffix
                .trim()
                .strip_prefix(':')
                .ok_or_else(|| format!("only pseudo-states may follow ::part() in '{target}'"))?
                .split(':')
            {
                if !matches!(
                    state,
                    "hover"
                        | "focus"
                        | "focus-visible"
                        | "active"
                        | "disabled"
                        | "checked"
                        | "windowed"
                        | "fullscreen"
                        | "maximized"
                        | "activated"
                        | "tiled"
                ) {
                    return Err(format!("unsupported part pseudo-state '{state}'"));
                }
                states.push(Selector::State("*".into(), state.into()));
            }
            Selector::Compound(states)
        };
        let body =
            css::parse_stylesheet(&format!("* {{ {block} }}"), 0).map_err(|err| err.to_string())?;
        let [Rule::Style(body)] = body.rules.as_slice() else {
            return Err(format!("expected declarations in '{target}'"));
        };
        rules.push(PartRule {
            module: module.into(),
            part: part.into(),
            state,
            declarations: body.declarations.clone(),
        });
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_styles_preserve_order_values_and_states() {
        let rules = parse_user_stylesheet("/* user */ @mesh/navigation-bar::part(glyph) { width: 20px; color: var(--color-primary); } @mesh/navigation-bar::part(glyph):hover:focus { opacity: .5; }").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].module, "@mesh/navigation-bar");
        assert_eq!(rules[0].part, "glyph");
        assert_eq!(rules[0].declarations[1].value, "var(--color-primary)");
        assert!(matches!(&rules[1].state, Selector::Compound(states) if states.len() == 2));
    }

    #[test]
    fn part_targets_accept_the_manifest_module_id_vocabulary() {
        for module in ["@123/4", "@alice.v2/bar.name", "@_scope/-name"] {
            let rules = parse_user_stylesheet(&format!("{module}::part(glyph) {{ opacity: .5; }}"))
                .unwrap();
            assert_eq!(rules[0].module, module);
        }
    }

    #[test]
    fn user_styles_reject_private_and_malformed_targets() {
        for source in [
            ".private { color: red; }",
            "button { width: 1px; }",
            "@mesh/foo::part() { width: 1px; }",
            "@mesh/foo::part(glyph) .private { width: 1px; }",
            "@mesh/foo::part(glyph):bogus { width: 1px; }",
            "@mesh/foo::part(glyph);",
            "@mesh/foo::part(glyph) { .private { width: 1px; } }",
            "@mesh/foo::part(glyph) { width: 1px;",
            "@import 'file.css';",
            "@mesh/foo::part(glyph) { --nested: { color: red; }",
            "@mesh/foo::part(glyph) { width: 1px; }}",
            "@mesh/foo::part(glyph) { width: 1px; } @mesh/foo::part(frame) {",
            "@mesh/foo::part(glyph) { width: 1px; } /* unfinished",
        ] {
            assert!(parse_user_stylesheet(source).is_err(), "{source}");
        }
    }
}
