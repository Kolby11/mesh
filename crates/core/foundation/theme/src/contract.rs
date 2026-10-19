use cssparser::{Parser, ParserInput, Token};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::OnceLock;

/// The newest semantic token contract this runtime knows.
pub const SEMANTIC_TOKEN_CONTRACT_VERSION: &str = "1.1";

const FIRST_TOKEN_CONTRACT_VERSION: &str = "1.0";

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
    /// Contract version that added the role; absent means 1.0.
    #[serde(default)]
    pub since: Option<String>,
    /// Platform value, written in older contract roles, used when the active
    /// theme omits the token. Roles added after 1.0 carry one so that themes
    /// declaring an older minor version remain complete.
    #[serde(default)]
    pub default: Option<String>,
}

impl SemanticToken {
    pub fn name(&self) -> String {
        super::canonical_token_name(&self.css)
    }

    pub fn since(&self) -> &str {
        self.since
            .as_deref()
            .unwrap_or(FIRST_TOKEN_CONTRACT_VERSION)
    }

    /// Whether a theme declaring `version` must supply this token itself.
    pub fn required_by(&self, version: &str) -> bool {
        self.source == "theme"
            && self.default.is_none()
            && compare_versions(self.since(), version).is_some_and(|order| order.is_le())
    }
}

fn parse_version(version: &str) -> Option<(u32, u32)> {
    let (major, minor) = version.trim().split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(parse_version(left)?.cmp(&parse_version(right)?))
}

/// A theme may declare any minor version of the current major contract up to
/// the newest one this runtime knows; newer roles fall back to their defaults.
pub fn is_supported_token_contract(version: &str) -> bool {
    let (Some(declared), Some(known)) = (
        parse_version(version),
        parse_version(SEMANTIC_TOKEN_CONTRACT_VERSION),
    ) else {
        return false;
    };
    declared.0 == known.0 && declared.1 <= known.1
}

/// Contract roles with a platform default, as `(dotted name, raw value)`.
pub fn semantic_token_defaults() -> impl Iterator<Item = (String, &'static str)> {
    semantic_token_contract()
        .tokens
        .iter()
        .filter_map(|token| Some((token.name(), token.default.as_deref()?)))
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

/// Vocabulary a theme declaring `version` must supply but whose stylesheet
/// omits from its root tokens. Font-role and icon-axis tokens are supplied by
/// resource composition; roles with a platform default are never missing.
pub fn missing_semantic_tokens(source: &str, version: &str) -> Result<Vec<String>, String> {
    let theme = super::parse_theme_css("contract-check", "Contract check", source)?;
    Ok(semantic_token_contract()
        .tokens
        .iter()
        .filter(|token| token.required_by(version) && !theme.tokens().contains_key(&token.name()))
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
    fn later_minor_roles_carry_defaults_in_earlier_vocabulary() {
        for token in &semantic_token_contract().tokens {
            if token.since() == FIRST_TOKEN_CONTRACT_VERSION {
                continue;
            }
            let default = token
                .default
                .as_deref()
                .unwrap_or_else(|| panic!("{} needs a default", token.css));
            for read in css_token_usage(default).references {
                let read = semantic_token_contract()
                    .tokens
                    .iter()
                    .find(|candidate| candidate.css == read.name)
                    .unwrap_or_else(|| panic!("{} default reads {}", token.css, read.name));
                assert!(
                    compare_versions(read.since(), token.since())
                        .unwrap()
                        .is_lt()
                );
            }
        }
        assert!(is_supported_token_contract("1.0"));
        assert!(is_supported_token_contract(SEMANTIC_TOKEN_CONTRACT_VERSION));
        assert!(!is_supported_token_contract("1.99"));
        assert!(!is_supported_token_contract("2.0"));
        assert!(!is_supported_token_contract("one"));
        let focus = semantic_token_contract()
            .tokens
            .iter()
            .find(|token| token.css == "--color-focus-ring")
            .unwrap();
        assert!(!focus.required_by(SEMANTIC_TOKEN_CONTRACT_VERSION));
        let primary = semantic_token_contract()
            .tokens
            .iter()
            .find(|token| token.css == "--color-primary")
            .unwrap();
        assert!(primary.required_by("1.0"));
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
