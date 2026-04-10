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
