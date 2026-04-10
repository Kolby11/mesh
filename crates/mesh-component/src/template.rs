/// Template AST — represents the markup structure of a component.

/// The template block containing the root node list.
#[derive(Debug, Clone)]
pub struct TemplateBlock {
    pub root: Vec<TemplateNode>,
}

/// A single node in the template tree.
#[derive(Debug, Clone)]
pub enum TemplateNode {
    /// An element like `<row>`, `<text>`, `<button>`.
    Element(ElementNode),
    /// Raw text content.
    Text(TextNode),
    /// An expression interpolation: `{{ variable }}`.
    Expr(ExprNode),
    /// Conditional rendering: `@if condition`.
    If(IfNode),
    /// List rendering: `@for item in list`.
    For(ForNode),
    /// A named slot for child content: `<slot name="..."/>`.
    Slot(SlotNode),
    /// A reference to another component: `<MyWidget prop="value"/>`.
    Component(ComponentRef),
}

/// An element node with a tag, attributes, and children.
#[derive(Debug, Clone)]
pub struct ElementNode {
    /// Tag name: `row`, `column`, `text`, `button`, `image`, `icon`, `box`, `input`, etc.
    pub tag: String,
    /// Attributes on this element.
    pub attributes: Vec<Attribute>,
    /// Child nodes.
    pub children: Vec<TemplateNode>,
}

/// A single attribute on an element.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: AttributeValue,
}

/// How an attribute value is bound.
#[derive(Debug, Clone)]
pub enum AttributeValue {
    /// Static string: `class="container"`.
    Static(String),
    /// Data binding: `:text="title"` — resolves from script state.
    Binding(String),
    /// Event handler: `@click="onTap"` — calls a script function.
    EventHandler(String),
}

/// Raw text between elements.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub content: String,
}

/// An interpolation expression: `{{ formatTime(time) }}`.
#[derive(Debug, Clone)]
pub struct ExprNode {
    pub expression: String,
}

/// Conditional block.
#[derive(Debug, Clone)]
pub struct IfNode {
    pub condition: String,
    pub then_children: Vec<TemplateNode>,
    pub else_children: Vec<TemplateNode>,
}

/// Loop block.
#[derive(Debug, Clone)]
pub struct ForNode {
    pub item_name: String,
    pub iterable: String,
    pub children: Vec<TemplateNode>,
}

/// A slot for projected content.
#[derive(Debug, Clone)]
pub struct SlotNode {
    pub name: Option<String>,
}

/// A reference to a child component.
#[derive(Debug, Clone)]
pub struct ComponentRef {
    pub name: String,
    pub props: Vec<Attribute>,
    pub children: Vec<TemplateNode>,
}
