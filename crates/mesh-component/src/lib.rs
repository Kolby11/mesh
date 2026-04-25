pub mod meta;
pub mod parser;
pub mod schema;
pub mod style;
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

pub use meta::{AccessibilityRole, MetaBlock};
pub use parser::{ParseError, parse_component};
pub use schema::{SchemaBlock, SchemaFieldDef};
pub use style::*;
pub use template::*;

use std::collections::HashMap;

/// A parsed `.mesh` single-file component.
#[derive(Debug, Clone)]
pub struct ComponentFile {
    /// Component imports declared with `import "plugin-id" as Alias` in the script block.
    /// Maps alias name → plugin ID. These are stripped from the script source before Luau sees it.
    pub imports: HashMap<String, String>,
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
    pub schema: Option<SchemaBlock>,
    pub meta: Option<MetaBlock>,
}

/// A script block with its language and source code.
#[derive(Debug, Clone)]
pub struct ScriptBlock {
    pub lang: ScriptLang,
    /// Transformed source — top-level `local x = val` declarations are rewritten
    /// to `mesh.state.set("x", val)` calls so the runtime can track them reactively.
    pub source: String,
    /// Variable names extracted from top-level `local` declarations.
    pub reactive_vars: Vec<String>,
}

/// Supported scripting languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLang {
    Luau,
}
