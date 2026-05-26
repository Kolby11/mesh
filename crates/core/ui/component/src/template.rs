/// Template AST — represents the markup structure of a component.

/// Source-level UI tag classification.
///
/// Encodes the semantic intent of the tag as written by the module author.
/// Distinct from `UiTag` in `mesh-core-render`, which is the lowered
/// runtime primitive set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceTag {
    // Layout family
    Panel,
    Row,
    Column,
    Grid,
    Stack,
    ScrollView,
    ScrollArea,
    Spacer,
    Divider,
    Separator,
    Section,
    Header,
    Footer,
    Group,
    FormRow,
    // Content family
    Text,
    Label,
    Icon,
    Image,
    Badge,
    Progress,
    Meter,
    Tooltip,
    Avatar,
    Shortcut,
    // Controls family
    Button,
    IconButton,
    ToggleButton,
    CommandButton,
    LinkButton,
    Input,
    TextArea,
    Search,
    Password,
    NumberInput,
    Stepper,
    TextInput,
    PasswordInput,
    SearchInput,
    EmailInput,
    UrlInput,
    Slider,
    Select,
    Option,
    Switch,
    Checkbox,
    Radio,
    RadioGroup,
    SegmentedControl,
    Menu,
    MenuItem,
    CommandItem,
    PreferenceRow,
    // Container family
    Popover,
    Dialog,
    Sheet,
    Tabs,
    Tab,
    Accordion,
    Details,
    // Structure family
    List,
    ListItem,
    Table,
    Cell,
    Tree,
    EmptyState,
    Slot,
    // Composition family
    Surface,
    Widget,
    Box,
    Scroll,
    // Unrecognized tag
    Unknown,
}

impl SourceTag {
    /// Classify a raw tag name from the template source.
    pub fn from_tag_name(tag: &str) -> Self {
        match tag {
            // Built-in MESH UI vocabulary. Keep primitives lowercase so
            // PascalCase remains unambiguous for custom components.
            "panel" => Self::Panel,
            "row" => Self::Row,
            "column" => Self::Column,
            "grid" => Self::Grid,
            "stack" => Self::Stack,
            "scroll-view" => Self::ScrollView,
            "scroll-area" => Self::ScrollArea,
            "scroll" => Self::Scroll,
            "spacer" => Self::Spacer,
            "divider" => Self::Divider,
            "separator" => Self::Separator,
            "section" => Self::Section,
            "header" => Self::Header,
            "footer" => Self::Footer,
            "group" => Self::Group,
            "form-row" => Self::FormRow,
            "box" => Self::Box,
            "text" => Self::Text,
            "label" => Self::Label,
            "icon" => Self::Icon,
            "image" => Self::Image,
            "badge" => Self::Badge,
            "progress" => Self::Progress,
            "meter" => Self::Meter,
            "tooltip" => Self::Tooltip,
            "avatar" => Self::Avatar,
            "shortcut" => Self::Shortcut,
            "button" => Self::Button,
            "icon-button" => Self::IconButton,
            "toggle-button" => Self::ToggleButton,
            "command-button" => Self::CommandButton,
            "link-button" => Self::LinkButton,
            "input" => Self::Input,
            "textarea" => Self::TextArea,
            "search" => Self::Search,
            "password" => Self::Password,
            "number-input" => Self::NumberInput,
            "stepper" => Self::Stepper,
            "text-input" => Self::TextInput,
            "password-input" => Self::PasswordInput,
            "search-input" => Self::SearchInput,
            "email-input" => Self::EmailInput,
            "url-input" => Self::UrlInput,
            "slider" => Self::Slider,
            "select" => Self::Select,
            "option" => Self::Option,
            "switch" => Self::Switch,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "radio-group" => Self::RadioGroup,
            "segmented-control" => Self::SegmentedControl,
            "menu" => Self::Menu,
            "menu-item" => Self::MenuItem,
            "command-item" => Self::CommandItem,
            "preference-row" => Self::PreferenceRow,
            "popover" => Self::Popover,
            "dialog" => Self::Dialog,
            "sheet" => Self::Sheet,
            "tabs" => Self::Tabs,
            "tab" => Self::Tab,
            "accordion" => Self::Accordion,
            "details" => Self::Details,
            "list" => Self::List,
            "list-item" => Self::ListItem,
            "table" => Self::Table,
            "cell" => Self::Cell,
            "tree" => Self::Tree,
            "empty-state" => Self::EmptyState,
            "slot" => Self::Slot,
            "surface" => Self::Surface,
            "widget" => Self::Widget,
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
    /// Mounted component instance binding: `bind:this={child}`.
    InstanceBinding(String),
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
