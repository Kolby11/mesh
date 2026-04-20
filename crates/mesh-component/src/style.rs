/// Style AST — CSS-like styling with theme token references.

/// The style block containing all rules for a component.
#[derive(Debug, Clone)]
pub struct StyleBlock {
    pub rules: Vec<StyleRule>,
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

/// A single CSS-like property declaration.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: StyleValue,
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
