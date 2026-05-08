/// Style AST — CSS-like styling with theme token references.

/// The style block containing all rules for a component.
#[derive(Debug, Clone)]
pub struct StyleBlock {
    pub rules: Vec<StyleRule>,
    pub keyframes: Vec<KeyframeRule>,
}

/// A single style rule: selector + declarations.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub container_query: Option<ContainerQuery>,
}

/// A simplified CSS container query evaluated against the current container size.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContainerQuery {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl ContainerQuery {
    pub fn matches(&self, width: f32, height: f32) -> bool {
        if let Some(min_width) = self.min_width {
            if width < min_width {
                return false;
            }
        }
        if let Some(max_width) = self.max_width {
            if width > max_width {
                return false;
            }
        }
        if let Some(min_height) = self.min_height {
            if height < min_height {
                return false;
            }
        }
        if let Some(max_height) = self.max_height {
            if height > max_height {
                return false;
            }
        }

        true
    }

    pub fn intersect(self, other: Self) -> Self {
        Self {
            min_width: match (self.min_width, other.min_width) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_width: match (self.max_width, other.max_width) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            min_height: match (self.min_height, other.min_height) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_height: match (self.max_height, other.max_height) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
        }
    }
}

/// A CSS-like selector.
#[derive(Debug, Clone)]
pub enum Selector {
    /// Tag selector: `button`.
    Tag(String),
    /// Class selector: `.container`.
    Class(String),
    /// ID selector: `#main`.
    Id(String),
    /// Pseudo-state: `button:hover`, `input:focused`.
    State(String, String),
    /// Multiple selectors combined: `button.primary`.
    Compound(Vec<Selector>),
    /// Universal selector: `*`.
    Universal,
}

/// A pre-compiled selector shape used at runtime by `StyleResolver`.
///
/// Lowered from `Selector` during the compile/lower stage. Only selectors that
/// MESH can afford to match at runtime are representable here; unsupported
/// selectors (descendant combinators, `:has()`, etc.) are rejected with a
/// diagnostic before reaching this type.
#[derive(Debug, Clone)]
pub enum LoweredSelector {
    Simple(SimpleSelector),
    State(SimpleSelector, StateSelector),
}

/// The structural part of a lowered selector: optional tag, optional id, class set.
#[derive(Debug, Clone, Default)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

/// Runtime state that a selector can match against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSelector {
    Hover,
    Focus,
    Active,
    Disabled,
    Checked,
}

/// A single CSS-like property declaration.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: StyleValue,
}

/// A named `@keyframes` rule parsed from a style block.
#[derive(Debug, Clone)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
}

/// A single percentage stop within a keyframe rule.
#[derive(Debug, Clone)]
pub struct KeyframeStop {
    pub offset: f32,
    pub declarations: Vec<Declaration>,
}

/// A style value that may reference theme tokens.
#[derive(Debug, Clone)]
pub enum StyleValue {
    /// A literal value: `#ff0000`, `16px`, `bold`.
    Literal(String),
    /// A theme token reference: `token(color.primary)`.
    Token(String),
    /// A variable reference: `var(--custom-prop)`.
    Var(String),
}

pub fn is_transition_safe_keyframe_property(property: &str) -> bool {
    matches!(
        property,
        "background"
            | "background-color"
            | "border-color"
            | "border-radius"
            | "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-right-radius"
            | "border-bottom-left-radius"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "color"
            | "opacity"
            | "width"
            | "height"
            | "min-width"
            | "max-width"
            | "min-height"
            | "max-height"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "padding-x"
            | "padding-y"
            | "padding-inline"
            | "padding-block"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "margin-x"
            | "margin-y"
            | "margin-inline"
            | "margin-block"
            | "transform"
            | "font-size"
            | "letter-spacing"
            | "line-height"
            | "gap"
            | "row-gap"
            | "column-gap"
            | "gap-x"
            | "inset"
            | "top"
            | "right"
            | "bottom"
            | "left"
    )
}
