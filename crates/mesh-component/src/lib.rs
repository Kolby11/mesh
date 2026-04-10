/// Single-file component parser for `.mesh` files.
///
/// A `.mesh` file contains up to six blocks:
///
/// ```text
/// <template>  — XHTML-like markup
/// <script>    — Luau logic
/// <style>     — CSS-like styling with theme token references
/// <schema>    — Typed settings schema (TOML)
/// <i18n>      — Translations keyed by locale
/// <meta>      — Component metadata and accessibility info
/// ```
///
/// This crate parses these blocks into a typed AST. It has no runtime
/// dependencies — it does not depend on mesh-theme, mesh-service, or
/// any other mesh crate.

pub mod template;
pub mod style;
pub mod schema;
pub mod i18n;
pub mod meta;
pub mod parser;

pub use parser::{parse_component, ParseError};
pub use template::*;
pub use style::*;
pub use schema::{SchemaBlock, SchemaFieldDef};
pub use i18n::I18nBlock;
pub use meta::{MetaBlock, AccessibilityRole};

/// A parsed `.mesh` single-file component.
#[derive(Debug, Clone)]
pub struct ComponentFile {
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
    pub schema: Option<SchemaBlock>,
    pub i18n: Option<I18nBlock>,
    pub meta: Option<MetaBlock>,
}

/// A script block with its language and source code.
#[derive(Debug, Clone)]
pub struct ScriptBlock {
    pub lang: ScriptLang,
    pub source: String,
}

/// Supported scripting languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLang {
    Luau,
}
