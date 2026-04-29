pub mod parser;
pub mod style;
/// Single-file component parser for `.mesh` files.
///
/// A `.mesh` file contains these blocks:
///
/// ```text
/// <template>  — XHTML-like markup
/// <script>    — Luau logic
/// <style>     — CSS-like styling with theme token references
/// <i18n>      — Translations keyed by locale
/// ```
///
/// This crate parses these blocks into a typed AST. It has no runtime
/// dependencies — it does not depend on mesh-theme, mesh-service, or
/// any other mesh crate.
pub mod template;

pub use parser::{ParseError, parse_component};
pub use style::*;
pub use template::*;

/// A parsed authoring-time import from a `.mesh` script block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentImport {
    pub alias: String,
    pub target: ComponentImportTarget,
}

/// Supported explicit import targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentImportTarget {
    /// A local component file, either relative to the importing file or `@src/...`.
    ComponentLocal(String),
    /// A frontend plugin ID, such as `@mesh/volume-bar`.
    ComponentPlugin(String),
    /// A MESH interface API, such as `mesh.audio` with an optional version requirement.
    InterfaceApi {
        interface: String,
        version: Option<String>,
    },
}

/// A parsed `.mesh` single-file component.
#[derive(Debug, Clone)]
pub struct ComponentFile {
    pub imports: Vec<ComponentImport>,
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
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
