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
/// ```
///
/// This crate parses these blocks into a typed AST. It has no runtime
/// dependencies — it does not depend on mesh-core-theme, mesh-core-service, or
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
    /// A frontend module ID, such as `@mesh/volume-bar`.
    ComponentModule(String),
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
    pub props: Option<PropsBlock>,
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
}

/// A parsed `<props>` block: the component's typed, defaulted configuration.
///
/// Each entry auto-projects to a `prop(name)` CSS reference, a reactive
/// `props.name` script field, and a generated settings-UI row. See
/// `docs/component-configuration.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsBlock {
    pub props: Vec<PropDef>,
}

/// A single declared prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: String,
    pub ty: PropType,
    pub default: Option<PropValue>,
    pub label: Option<LocalizedLabel>,
    pub description: Option<LocalizedLabel>,
    /// Allowed values for `enum` props.
    pub options: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub unit: Option<String>,
    /// Whether the prop appears in the generated settings UI (default `true`).
    pub expose: bool,
}

/// The validated value domain of a prop. Drives CSS projection, the Lua value
/// kind, the generated settings control, and use-site type checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    Size,
    Number,
    Int,
    Bool,
    Enum,
    String,
    Color,
    Token,
    Duration,
    Icon,
}

impl PropType {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "size" => Self::Size,
            "number" => Self::Number,
            "int" => Self::Int,
            "bool" => Self::Bool,
            "enum" => Self::Enum,
            "string" => Self::String,
            "color" => Self::Color,
            "token" => Self::Token,
            "duration" => Self::Duration,
            "icon" => Self::Icon,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Number => "number",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum => "enum",
            Self::String => "string",
            Self::Color => "color",
            Self::Token => "token",
            Self::Duration => "duration",
            Self::Icon => "icon",
        }
    }
}

/// A scalar prop value (used for `default`).
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    String(String),
    Number(f64),
    Bool(bool),
}

/// A user-facing prop label/description: a literal or an i18n reference.
///
/// Mirrors `LocalizedText` in `mesh-core-module`; kept independent here so the
/// component crate stays free of runtime dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalizedLabel {
    Literal(String),
    Translation {
        key: String,
        fallback: Option<String>,
    },
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
