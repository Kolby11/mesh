use cssparser::{Parser, ParserInput, Token};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::OnceLock;

pub const SEMANTIC_TOKEN_CONTRACT_VERSION: &str = "1.0";

#[derive(Debug, Deserialize)]
pub struct SemanticTokenContract {
    pub version: String,
    pub tokens: Vec<SemanticToken>,
}

#[derive(Debug, Deserialize)]
pub struct SemanticToken {
    pub css: String,
    pub kind: String,
    pub source: String,
}

impl SemanticToken {
    pub fn name(&self) -> String {
        super::canonical_token_name(&self.css)
    }
}

pub fn semantic_token_contract() -> &'static SemanticTokenContract {
    static CONTRACT: OnceLock<SemanticTokenContract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("semantic-tokens-v1.json"))
            .expect("bundled semantic token contract")
    })
}

pub fn is_standard_token(name: &str) -> bool {
    static NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            semantic_token_contract()
                .tokens
                .iter()
                .map(SemanticToken::name)
                .collect()
        })
        .contains(&super::canonical_token_name(name))
}

pub fn is_private_token(name: &str) -> bool {
    let name = super::canonical_token_name(name);
    let Some((module, _)) = super::split_explicit_module_token(&name) else {
        return false;
    };
    let Some((publisher, unit)) = module
        .strip_prefix('@')
        .and_then(|module| module.split_once('/'))
    else {
        return false;
    };
    !publisher.is_empty()
        && !unit.is_empty()
        && publisher.chars().chain(unit.chars()).all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssTokenReference {
    pub name: String,
    pub span: Range<usize>,
}

#[derive(Debug, Default)]
pub struct CssTokenUsage {
    pub references: Vec<CssTokenReference>,
    pub declarations: BTreeSet<String>,
}

/// Find var() reads and local custom-property declarations using CSS tokens.
/// Spans are byte offsets in source; comments, strings and URLs are ignored.
pub fn css_token_usage(source: &str) -> CssTokenUsage {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut usage = CssTokenUsage::default();
    scan(&mut parser, &mut usage, 0);
    usage
}

fn scan(parser: &mut Parser<'_, '_>, usage: &mut CssTokenUsage, depth: usize) {
    if depth >= 128 {
        return;
    }
    while let Ok(token) = parser.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                let _: Result<(), cssparser::ParseError<'_, ()>> =
                    parser.parse_nested_block(|inner| {
                        inner.skip_whitespace();
                        let start = inner.position().byte_index();
                        if let Ok(Token::Ident(name)) = inner.next().cloned() {
                            if name.starts_with("--") {
                                usage.references.push(CssTokenReference {
                                    name: name.to_string(),
                                    span: start..inner.position().byte_index(),
                                });
                            }
                        }
                        scan(inner, usage, depth + 1);
                        Ok(())
                    });
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                let _: Result<(), cssparser::ParseError<'_, ()>> =
                    parser.parse_nested_block(|inner| {
                        scan(inner, usage, depth + 1);
                        Ok(())
                    });
            }
            Token::Ident(name) if name.starts_with("--") => {
                let state = parser.state();
                if matches!(parser.next(), Ok(Token::Colon)) {
                    usage
                        .declarations
                        .insert(super::canonical_token_name(&name));
                }
                parser.reset(&state);
            }
            _ => {}
        }
    }
}

pub fn token_is_declared_locally(name: &str, declarations: &BTreeSet<String>) -> bool {
    declarations.contains(&super::canonical_token_name(name))
}

/// Required theme-supplied vocabulary absent from the stylesheet's root tokens.
/// Font-role and icon-axis tokens are supplied by resource composition.
pub fn missing_semantic_tokens(source: &str) -> Result<Vec<String>, String> {
    let theme = super::parse_theme_css("contract-check", "Contract check", source)?;
    Ok(semantic_token_contract()
        .tokens
        .iter()
        .filter(|token| token.source == "theme" && !theme.tokens().contains_key(&token.name()))
        .map(SemanticToken::name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_is_unique_versioned_and_independent_of_palette_tokens() {
        let contract = semantic_token_contract();
        assert_eq!(contract.version, SEMANTIC_TOKEN_CONTRACT_VERSION);
        let names = contract
            .tokens
            .iter()
            .map(SemanticToken::name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), contract.tokens.len());
        for name in [
            "--color-primary",
            "color.on-surface",
            "--font-body",
            "--icon-weight",
            "--typography-scale-label-small-size",
        ] {
            assert!(is_standard_token(name), "{name}");
        }
        assert!(!is_standard_token("--base16-base00"));
        assert!(!is_standard_token("--animation-default-opacity"));
        assert!(!is_private_token("@unqualified"));
        assert!(is_private_token("@alice/widget.color.accent"));
    }

    #[test]
    fn css_reads_handle_fallbacks_escapes_nested_blocks_and_trivia() {
        let source = r#"/* var(--comment) */ .box {
--local: 4px; color: var(--color\2d primary, var(--unknown));
width: calc(var(--local) * 2); content: "var(--string)";
background: url("var(--url)"); }"#;
        let usage = css_token_usage(source);
        assert_eq!(
            usage
                .references
                .iter()
                .map(|read| read.name.as_str())
                .collect::<Vec<_>>(),
            ["--color-primary", "--unknown", "--local"]
        );
        assert!(token_is_declared_locally("--local", &usage.declarations));
        assert_eq!(&source[usage.references[1].span.clone()], "--unknown");
    }
}
