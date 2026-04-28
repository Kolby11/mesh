/// Template AST — represents the markup structure of a component.

/// Source-level UI tag classification.
///
/// Encodes the semantic intent of the tag as written by the plugin author.
/// Distinct from `UiTag` in `mesh-render-engine`, which is the lowered
/// runtime primitive set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceTag {
    // Layout family
    Panel,
    Row,
    Column,
    Stack,
    ScrollView,
    Spacer,
    Separator,
    // Content family
    Text,
    Label,
    Icon,
    Image,
    // Controls family
    Button,
    IconButton,
    Input,
    TextInput,
    PasswordInput,
    SearchInput,
    NumberInput,
    EmailInput,
    UrlInput,
    Slider,
    Switch,
    Checkbox,
    // Structure family
    List,
    ListItem,
    Slot,
    // Composition family
    Surface,
    Widget,
    // Legacy lowercase tags (current plugin authoring convention)
    LegacyBox,
    LegacyScroll,
    // Unrecognized tag
    Unknown,
}

impl SourceTag {
    /// Classify a raw tag name from the template source.
    pub fn from_tag_name(tag: &str) -> Self {
        match tag {
            // MESH UI vocabulary
            "Panel" => Self::Panel,
            "Row" => Self::Row,
            "Column" => Self::Column,
            "Stack" => Self::Stack,
            "ScrollView" => Self::ScrollView,
            "Spacer" => Self::Spacer,
            "Separator" => Self::Separator,
            "Text" => Self::Text,
            "Label" => Self::Label,
            "Icon" => Self::Icon,
            "Image" => Self::Image,
            "Button" => Self::Button,
            "IconButton" => Self::IconButton,
            "Input" => Self::Input,
            "TextInput" => Self::TextInput,
            "PasswordInput" => Self::PasswordInput,
            "SearchInput" => Self::SearchInput,
            "NumberInput" => Self::NumberInput,
            "EmailInput" => Self::EmailInput,
            "UrlInput" => Self::UrlInput,
            "Slider" => Self::Slider,
            "Switch" => Self::Switch,
            "Checkbox" => Self::Checkbox,
            "List" => Self::List,
            "ListItem" => Self::ListItem,
            "Slot" => Self::Slot,
            "Surface" => Self::Surface,
            "Widget" => Self::Widget,
            // Legacy lowercase (existing plugin convention)
            "row" => Self::Row,
            "column" => Self::Column,
            "text" => Self::Text,
            "button" => Self::Button,
            "input" => Self::Input,
            "slider" => Self::Slider,
            "icon" => Self::Icon,
            "scroll" => Self::LegacyScroll,
            "box" => Self::LegacyBox,
            "label" => Self::Label,
            "image" => Self::Image,
            "list" => Self::List,
            "separator" => Self::Separator,
            "spacer" => Self::Spacer,
            // Component refs are handled before ElementNode is constructed
            _ => Self::Unknown,
        }
    }
}

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
    /// An expression interpolation: `{ variable }`.
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
    /// Raw tag name as written in the source.
    pub tag: String,
    /// Semantic classification of the source tag.
    pub tag_kind: SourceTag,
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
    /// Dynamic binding: `title="{audio.tooltip}"` — expression resolved from script state.
    Binding(String),
    /// Two-way binding: `bind:value="volume"` — reads from and writes back to script state.
    TwoWayBinding(String),
    /// Event handler: `onclick="onTap"` — calls a script function.
    EventHandler(String),
}

/// Raw text between elements.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub content: String,
}

/// An interpolation expression: `{ formatTime(time) }`.
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
