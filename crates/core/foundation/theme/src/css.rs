//! Restricted CSS selector syntax shared by component and theme lowering.

use cssparser::{Parser, ParserInput, ToCss, Token};
use serde::{Deserialize, Serialize};

/// The selector subset MESH can match against a single element node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Selector {
    Tag(String),
    Class(String),
    Id(String),
    State(String, String),
    Compound(Vec<Selector>),
    Universal,
}

/// Parse one selector and reject combinators, functions, attributes, and
/// other CSS syntax for which the retained element tree has no matcher.
pub fn parse_selector(source: &str) -> Result<Selector, String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut parts = Vec::new();

    while let Ok(token) = parser.next() {
        match token {
            Token::Delim('*') => parts.push(Selector::Universal),
            Token::Delim('.') => {
                let class = parser
                    .expect_ident_cloned()
                    .map_err(|error| format!("{error:?}"))?;
                parts.push(Selector::Class(class.to_string()));
            }
            Token::IDHash(id) => parts.push(Selector::Id(id.to_string())),
            Token::Colon => {
                let state = parser
                    .expect_ident_cloned()
                    .map_err(|error| format!("{error:?}"))?;
                match parts.pop() {
                    Some(Selector::Tag(tag)) => {
                        parts.push(Selector::State(tag, state.to_string()));
                    }
                    Some(previous) => {
                        parts.push(previous);
                        parts.push(Selector::State("*".into(), state.to_string()));
                    }
                    None => parts.push(Selector::State("*".into(), state.to_string())),
                }
            }
            Token::Ident(tag) => parts.push(Selector::Tag(tag.to_string())),
            Token::WhiteSpace(_) => {
                return Err("descendant and sibling combinators are not supported".into());
            }
            other => {
                return Err(format!(
                    "unsupported selector token {}",
                    other.to_css_string()
                ));
            }
        }
    }

    if parts.is_empty() {
        return Err("empty selector".into());
    }
    if parts.len() == 1 {
        Ok(parts.remove(0))
    } else {
        Ok(Selector::Compound(parts))
    }
}
