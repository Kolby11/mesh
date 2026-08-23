//! Template AST — represents the markup structure of a component.

/// The tag's semantic intent as authored. Distinct from `UiTag` in
/// `mesh-core-render`, which is the lowered runtime primitive set.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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
    /// Return the canonical lowercase source spelling used by element
    /// contracts and compiler diagnostics.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::Row => "row",
            Self::Column => "column",
            Self::Grid => "grid",
            Self::Stack => "stack",
            Self::ScrollView => "scroll-view",
            Self::ScrollArea => "scroll-area",
            Self::Spacer => "spacer",
            Self::Divider => "divider",
            Self::Separator => "separator",
            Self::Section => "section",
            Self::Header => "header",
            Self::Footer => "footer",
            Self::Group => "group",
            Self::FormRow => "form-row",
            Self::Text => "text",
            Self::Label => "label",
            Self::Icon => "icon",
            Self::Image => "image",
            Self::Badge => "badge",
            Self::Progress => "progress",
            Self::Meter => "meter",
            Self::Tooltip => "tooltip",
            Self::Avatar => "avatar",
            Self::Shortcut => "shortcut",
            Self::Button => "button",
            Self::IconButton => "icon-button",
            Self::ToggleButton => "toggle-button",
            Self::CommandButton => "command-button",
            Self::LinkButton => "link-button",
            Self::Input => "input",
            Self::TextArea => "textarea",
            Self::Search => "search",
            Self::Password => "password",
            Self::NumberInput => "number-input",
            Self::Stepper => "stepper",
            Self::TextInput => "text-input",
            Self::PasswordInput => "password-input",
            Self::SearchInput => "search-input",
            Self::EmailInput => "email-input",
            Self::UrlInput => "url-input",
            Self::Slider => "slider",
            Self::Select => "select",
            Self::Option => "option",
            Self::Switch => "switch",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio",
            Self::RadioGroup => "radio-group",
            Self::SegmentedControl => "segmented-control",
            Self::Menu => "menu",
            Self::MenuItem => "menu-item",
            Self::CommandItem => "command-item",
            Self::PreferenceRow => "preference-row",
            Self::Popover => "popover",
            Self::Dialog => "dialog",
            Self::Sheet => "sheet",
            Self::Tabs => "tabs",
            Self::Tab => "tab",
            Self::Accordion => "accordion",
            Self::Details => "details",
            Self::List => "list",
            Self::ListItem => "list-item",
            Self::Table => "table",
            Self::Cell => "cell",
            Self::Tree => "tree",
            Self::EmptyState => "empty-state",
            Self::Slot => "slot",
            Self::Surface => "surface",
            Self::Widget => "widget",
            Self::Box => "box",
            Self::Scroll => "scroll",
            Self::Unknown => "unknown",
        }
    }

    /// Compatibility name for callers that used the former element-kind
    /// enum. Generated element contracts remain the runtime source of truth.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Box | Self::Unknown => "MeshElement",
            Self::Row => "RowElement",
            Self::Column => "ColumnElement",
            Self::Grid => "GridElement",
            Self::Stack => "StackElement",
            Self::Scroll | Self::ScrollView | Self::ScrollArea => "ScrollElement",
            Self::Spacer => "SpacerElement",
            Self::Divider | Self::Separator => "SeparatorElement",
            Self::Section => "SectionElement",
            Self::Header => "HeaderElement",
            Self::Footer => "FooterElement",
            Self::Group => "GroupElement",
            Self::FormRow => "FormRowElement",
            Self::Text | Self::Badge | Self::Shortcut => "TextElement",
            Self::Label => "LabelElement",
            Self::Icon => "IconElement",
            Self::Image => "ImageElement",
            Self::Progress => "ProgressElement",
            Self::Meter => "MeterElement",
            Self::Tooltip => "TooltipElement",
            Self::Avatar => "AvatarElement",
            Self::Button | Self::CommandButton | Self::LinkButton => "ButtonElement",
            Self::IconButton => "IconButtonElement",
            Self::ToggleButton => "ToggleButtonElement",
            Self::Input
            | Self::TextArea
            | Self::Search
            | Self::Password
            | Self::NumberInput
            | Self::Stepper
            | Self::TextInput
            | Self::PasswordInput
            | Self::SearchInput
            | Self::EmailInput
            | Self::UrlInput => "InputElement",
            Self::Slider => "SliderElement",
            Self::Select => "SelectElement",
            Self::Option => "OptionElement",
            Self::Switch => "SwitchElement",
            Self::Checkbox | Self::Radio => "CheckboxElement",
            Self::RadioGroup => "RadioGroupElement",
            Self::SegmentedControl => "SegmentedControlElement",
            Self::Menu => "MenuElement",
            Self::MenuItem | Self::CommandItem => "MenuItemElement",
            Self::PreferenceRow => "PreferenceRowElement",
            Self::Panel => "PanelElement",
            Self::Popover => "PopoverElement",
            Self::Dialog => "DialogElement",
            Self::Sheet => "SheetElement",
            Self::Tabs => "TabsElement",
            Self::Tab => "TabElement",
            Self::Accordion => "AccordionElement",
            Self::Details => "DetailsElement",
            Self::List => "ListElement",
            Self::ListItem => "ListItemElement",
            Self::Table => "TableElement",
            Self::Cell => "CellElement",
            Self::Tree => "TreeElement",
            Self::EmptyState => "EmptyStateElement",
            Self::Slot => "SlotElement",
            Self::Surface => "SurfaceElement",
            Self::Widget => "WidgetElement",
        }
    }

    pub fn from_tag_name(tag: &str) -> Self {
        match tag {
            // Primitives stay lowercase so PascalCase is unambiguously a
            // custom component.
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
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TemplateBlock {
    pub root: Vec<TemplateNode>,
    /// The template body, excluding the surrounding block tags.
    pub span: crate::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum TemplateNode {
    Element(ElementNode),
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
    /// The complete element, including its opening and closing tags.
    pub span: crate::SourceSpan,
}

/// A single attribute on an element.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: AttributeValue,
    /// The complete authored attribute, including its name and value.
    pub span: Option<crate::SourceSpan>,
}

/// How an attribute value is bound.
#[derive(Debug, Clone)]
pub enum AttributeValue {
    /// Static string: `class="container"`.
    Static(String),
    /// Dynamic binding: `title={audio.tooltip}` — expression resolved from script state.
    Binding(String),
    /// Two-way binding: `bind:value="volume"` — reads from and writes back to script state.
    TwoWayBinding(String),
    /// Mounted component instance binding: `bind:this={child}`.
    InstanceBinding(String),
    /// Event handler: `onclick="onTap"` — calls a script function.
    EventHandler(String),
    /// Event handler with pre-bound arguments: `onclick={selectLocale(locale)}`.
    EventHandlerCall { handler: String, args: Vec<String> },
}

/// Raw text between elements.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub content: String,
    pub span: crate::SourceSpan,
}

/// An interpolation expression: `{ formatTime(time) }`.
#[derive(Debug, Clone)]
pub struct ExprNode {
    pub expression: String,
    /// The complete interpolation, including its braces.
    pub span: crate::SourceSpan,
    /// The expression body without surrounding braces or whitespace.
    pub expression_span: crate::SourceSpan,
}

/// Conditional block.
#[derive(Debug, Clone)]
pub struct IfNode {
    pub condition: String,
    /// The complete control-flow block, from `{#if` through `{/if}`.
    pub span: crate::SourceSpan,
    /// The condition body without surrounding directive syntax.
    pub condition_span: crate::SourceSpan,
    pub then_children: Vec<TemplateNode>,
    pub else_children: Vec<TemplateNode>,
}

/// Loop block.
#[derive(Debug, Clone)]
pub struct ForNode {
    pub item_name: String,
    pub iterable: String,
    /// The complete control-flow block, from `{#for` through `{/for}`.
    pub span: crate::SourceSpan,
    /// The iterable expression body in the opening directive.
    pub iterable_span: crate::SourceSpan,
    /// Optional expression that gives each iteration a stable identity.
    pub key: Option<String>,
    /// The optional key expression body in the opening directive.
    pub key_span: Option<crate::SourceSpan>,
    pub children: Vec<TemplateNode>,
}

/// A slot for projected content.
#[derive(Debug, Clone)]
pub struct SlotNode {
    /// The extension point contract name this slot hosts. Slots are keyed by
    /// contract, never by module id, so a host can be replaced without
    /// breaking its contributors.
    pub extension_point: Option<String>,
    /// Stable component-local address for a user-configurable slot.
    pub name: Option<String>,
    /// True selects placements from the active composition/profile instead of
    /// automatically rendering every compatible contribution.
    pub customizable: bool,
    pub span: crate::SourceSpan,
}

/// A reference to a child component.
#[derive(Debug, Clone)]
pub struct ComponentRef {
    pub name: String,
    /// Source-order identity within this parsed template.
    pub source_ordinal: usize,
    /// Zero-based source-order identity when this alias occurs more than once
    /// in the same template. Unique aliases keep `None` for stable legacy keys.
    pub duplicate_ordinal: Option<usize>,
    /// This reference is nested under a `for` and may render more than once.
    pub repeated_by_loop: bool,
    pub props: Vec<Attribute>,
    pub children: Vec<TemplateNode>,
    /// The complete component element, including its children.
    pub span: crate::SourceSpan,
}
