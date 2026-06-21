//! Core element model exposed to runtime code and tooling.
//!
//! Elements are MESH-owned primitives (`button`, `icon`, `input`, etc.).
//! Components compose these primitives; modules package complete features.

use crate::{AccessibilityRole, ElementState, WidgetNode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementKind {
    Box,
    Row,
    Column,
    Grid,
    Stack,
    Scroll,
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
    Panel,
    Popover,
    Dialog,
    Sheet,
    Tabs,
    Tab,
    Accordion,
    Details,
    List,
    ListItem,
    Table,
    Cell,
    Tree,
    EmptyState,
    Slot,
    Surface,
    Widget,
    Unknown,
}

impl ElementKind {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "box" => Self::Box,
            "row" => Self::Row,
            "column" => Self::Column,
            "grid" => Self::Grid,
            "stack" => Self::Stack,
            "scroll" => Self::Scroll,
            "scroll-view" => Self::ScrollView,
            "scroll-area" => Self::ScrollArea,
            "spacer" => Self::Spacer,
            "divider" => Self::Divider,
            "separator" => Self::Separator,
            "section" => Self::Section,
            "header" => Self::Header,
            "footer" => Self::Footer,
            "group" => Self::Group,
            "form-row" => Self::FormRow,
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
            "panel" => Self::Panel,
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

    pub const fn type_name(self) -> &'static str {
        match self {
            Self::Icon => "IconElement",
            Self::Image => "ImageElement",
            Self::Text | Self::Badge | Self::Shortcut => "TextElement",
            Self::Label => "LabelElement",
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
            | Self::Stepper => "InputElement",
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
            Self::Row => "RowElement",
            Self::Column => "ColumnElement",
            Self::Grid => "GridElement",
            Self::Stack => "StackElement",
            Self::Scroll | Self::ScrollView | Self::ScrollArea => "ScrollElement",
            Self::Spacer => "SpacerElement",
            Self::Separator | Self::Divider => "SeparatorElement",
            Self::Section => "SectionElement",
            Self::Header => "HeaderElement",
            Self::Footer => "FooterElement",
            Self::Group => "GroupElement",
            Self::FormRow => "FormRowElement",
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
            Self::Box | Self::Unknown => "MeshElement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementFamily {
    Layout,
    Display,
    Action,
    TextInput,
    ChoiceMenu,
    Container,
    Collection,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementAttributeType {
    String,
    Number,
    Boolean,
    Token,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementAttributeDef {
    pub name: &'static str,
    pub attribute_type: ElementAttributeType,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementStateFlag {
    Disabled,
    ReadOnly,
    Required,
    Focused,
    Selected,
    Checked,
    Expanded,
    Pressed,
    Invalid,
    Active,
    Value,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementEventDef {
    pub name: &'static str,
    pub payload: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct ElementAccessibilityDef {
    pub role: AccessibilityRole,
    pub focusable: bool,
    pub label_required: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementCompatibilityRef {
    pub source: &'static str,
    pub reference: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone)]
pub struct ElementContractDef {
    pub kind: ElementKind,
    pub tag: &'static str,
    pub family: ElementFamily,
    pub type_name: &'static str,
    pub attributes: &'static [ElementAttributeDef],
    pub states: &'static [ElementStateFlag],
    pub events: &'static [ElementEventDef],
    pub accessibility: ElementAccessibilityDef,
    pub style_hooks: &'static [&'static str],
    pub compatibility: &'static [ElementCompatibilityRef],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementDiagnosticKind {
    UnsupportedAttribute,
    UnsupportedEvent,
    InvalidAttributeValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementDiagnostic {
    pub tag: String,
    pub name: String,
    pub kind: ElementDiagnosticKind,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementFieldType {
    String,
    Number,
    Boolean,
    Rect,
    Object,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementFieldDef {
    pub name: &'static str,
    pub field_type: ElementFieldType,
    pub optional: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementTypeDef {
    pub kind: ElementKind,
    pub tag: &'static str,
    pub type_name: &'static str,
    pub fields: &'static [ElementFieldDef],
}

pub static BASE_ELEMENT_FIELDS: &[ElementFieldDef] = &[
    field(
        "key",
        ElementFieldType::String,
        false,
        "Runtime element key",
    ),
    field(
        "id",
        ElementFieldType::String,
        true,
        "Template id attribute",
    ),
    field(
        "ref",
        ElementFieldType::String,
        true,
        "Template ref attribute",
    ),
    field(
        "tag",
        ElementFieldType::String,
        false,
        "Runtime element tag",
    ),
    field(
        "element_type",
        ElementFieldType::String,
        false,
        "Lua element type name",
    ),
    field("x", ElementFieldType::Number, false, "Left coordinate"),
    field("y", ElementFieldType::Number, false, "Top coordinate"),
    field("left", ElementFieldType::Number, false, "Left coordinate"),
    field("top", ElementFieldType::Number, false, "Top coordinate"),
    field("right", ElementFieldType::Number, false, "Right coordinate"),
    field(
        "bottom",
        ElementFieldType::Number,
        false,
        "Bottom coordinate",
    ),
    field("width", ElementFieldType::Number, false, "Element width"),
    field("height", ElementFieldType::Number, false, "Element height"),
    field(
        "client_width",
        ElementFieldType::Number,
        false,
        "Width after padding",
    ),
    field(
        "client_height",
        ElementFieldType::Number,
        false,
        "Height after padding",
    ),
    field(
        "bounding_client_rect",
        ElementFieldType::Rect,
        false,
        "Outer bounds",
    ),
    field(
        "client_bound_rect",
        ElementFieldType::Rect,
        false,
        "Inner content bounds",
    ),
    field(
        "scroll_x",
        ElementFieldType::Number,
        false,
        "Horizontal scroll",
    ),
    field(
        "scroll_y",
        ElementFieldType::Number,
        false,
        "Vertical scroll",
    ),
    field(
        "scroll_left",
        ElementFieldType::Number,
        false,
        "Horizontal scroll offset (DOM scrollLeft; alias of scroll_x)",
    ),
    field(
        "scroll_top",
        ElementFieldType::Number,
        false,
        "Vertical scroll offset (DOM scrollTop; alias of scroll_y)",
    ),
    field(
        "scroll_width",
        ElementFieldType::Number,
        false,
        "Full scrollable content width",
    ),
    field(
        "scroll_height",
        ElementFieldType::Number,
        false,
        "Full scrollable content height",
    ),
    field(
        "max_scroll_left",
        ElementFieldType::Number,
        false,
        "Maximum horizontal scroll offset",
    ),
    field(
        "max_scroll_top",
        ElementFieldType::Number,
        false,
        "Maximum vertical scroll offset",
    ),
    field(
        "hovered",
        ElementFieldType::Boolean,
        false,
        "Pointer hover state",
    ),
    field("active", ElementFieldType::Boolean, false, "Pressed state"),
    field(
        "focused",
        ElementFieldType::Boolean,
        false,
        "Keyboard focus state",
    ),
    field(
        "disabled",
        ElementFieldType::Boolean,
        false,
        "Disabled state",
    ),
    field("checked", ElementFieldType::Boolean, false, "Checked state"),
    field(
        "attributes",
        ElementFieldType::Object,
        false,
        "Resolved raw attributes",
    ),
];

static ICON_FIELDS: &[ElementFieldDef] = &[
    field("name", ElementFieldType::String, true, "Icon theme name"),
    field("src", ElementFieldType::String, true, "Icon file path"),
    field("size", ElementFieldType::Number, true, "Icon size hint"),
    field(
        "alt",
        ElementFieldType::String,
        true,
        "Accessible alternate text",
    ),
];

static TEXT_FIELDS: &[ElementFieldDef] = &[
    field(
        "content",
        ElementFieldType::String,
        true,
        "Resolved text content",
    ),
    field(
        "selectable",
        ElementFieldType::Boolean,
        true,
        "Whether text can be selected",
    ),
];

static BUTTON_FIELDS: &[ElementFieldDef] = &[
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
    field("variant", ElementFieldType::String, true, "Visual variant"),
];

static INPUT_FIELDS: &[ElementFieldDef] = &[
    field(
        "value",
        ElementFieldType::String,
        true,
        "Current input value",
    ),
    field(
        "placeholder",
        ElementFieldType::String,
        true,
        "Placeholder text",
    ),
    field("type", ElementFieldType::String, true, "Input type"),
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
    field(
        "readonly",
        ElementFieldType::Boolean,
        true,
        "Read-only state",
    ),
];

static SLIDER_FIELDS: &[ElementFieldDef] = &[
    field(
        "value",
        ElementFieldType::Number,
        true,
        "Current slider value",
    ),
    field("min", ElementFieldType::Number, true, "Minimum value"),
    field("max", ElementFieldType::Number, true, "Maximum value"),
    field("step", ElementFieldType::Number, true, "Step size"),
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
];

static CHECKABLE_FIELDS: &[ElementFieldDef] = &[
    field("checked", ElementFieldType::Boolean, true, "Checked state"),
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
];

static IMAGE_FIELDS: &[ElementFieldDef] = &[
    field("src", ElementFieldType::String, true, "Image source path"),
    field(
        "alt",
        ElementFieldType::String,
        true,
        "Accessible alternate text",
    ),
];

static LABEL_FIELDS: &[ElementFieldDef] = &[field(
    "for",
    ElementFieldType::String,
    true,
    "Associated input id",
)];

static COMMON_ATTRIBUTES: &[ElementAttributeDef] = &[
    attr("id", ElementAttributeType::String, "Template id attribute"),
    attr("class", ElementAttributeType::String, "Style class list"),
    attr("style", ElementAttributeType::String, "Inline style rules"),
    attr(
        "ref",
        ElementAttributeType::String,
        "Template ref attribute",
    ),
    attr(
        "data-mesh-element",
        ElementAttributeType::String,
        "Original source element tag before runtime lowering",
    ),
    attr("label", ElementAttributeType::String, "Accessible label"),
    attr(
        "aria-label",
        ElementAttributeType::String,
        "Accessible label",
    ),
    attr("role", ElementAttributeType::String, "Accessibility role"),
    attr(
        "aria-role",
        ElementAttributeType::String,
        "Accessibility role override",
    ),
    attr("title", ElementAttributeType::String, "Accessible title"),
    attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
    attr("busy", ElementAttributeType::Boolean, "Busy state"),
    attr(
        "default",
        ElementAttributeType::Boolean,
        "Default action state",
    ),
    attr(
        "destructive",
        ElementAttributeType::Boolean,
        "Destructive action state",
    ),
    attr("readonly", ElementAttributeType::Boolean, "Read-only state"),
    attr("required", ElementAttributeType::Boolean, "Required state"),
    attr("value", ElementAttributeType::String, "Current value"),
    attr("min", ElementAttributeType::Number, "Minimum value"),
    attr("max", ElementAttributeType::Number, "Maximum value"),
    attr("checked", ElementAttributeType::Boolean, "Checked state"),
    attr("selected", ElementAttributeType::Boolean, "Selected state"),
    attr("expanded", ElementAttributeType::Boolean, "Expanded state"),
    attr("open", ElementAttributeType::Boolean, "Open state"),
    attr("pressed", ElementAttributeType::Boolean, "Pressed state"),
    attr("invalid", ElementAttributeType::Boolean, "Invalid state"),
    attr("hidden", ElementAttributeType::Boolean, "Hidden state"),
    attr(
        "keybind",
        ElementAttributeType::String,
        "Associated keybind id or display shortcut",
    ),
    attr(
        "command",
        ElementAttributeType::String,
        "Command intent metadata",
    ),
    attr("href", ElementAttributeType::String, "Link intent metadata"),
    attr("type", ElementAttributeType::String, "Input type metadata"),
    attr(
        "placeholder",
        ElementAttributeType::String,
        "Input placeholder text",
    ),
    attr(
        "multiline",
        ElementAttributeType::Boolean,
        "Input accepts multiple lines",
    ),
    attr(
        "masked",
        ElementAttributeType::Boolean,
        "Input masks displayed text",
    ),
    attr("step", ElementAttributeType::Number, "Numeric step size"),
    attr("align", ElementAttributeType::String, "Layout alignment"),
    attr(
        "justify",
        ElementAttributeType::String,
        "Main-axis layout justification",
    ),
    attr("spacing", ElementAttributeType::Number, "Layout spacing"),
    attr("gap", ElementAttributeType::Number, "Layout gap"),
    attr("width", ElementAttributeType::String, "Requested width"),
    attr("height", ElementAttributeType::String, "Requested height"),
    attr("min-width", ElementAttributeType::String, "Minimum width"),
    attr("max-width", ElementAttributeType::String, "Maximum width"),
    attr("min-height", ElementAttributeType::String, "Minimum height"),
    attr("max-height", ElementAttributeType::String, "Maximum height"),
    attr(
        "overflow",
        ElementAttributeType::String,
        "Overflow behavior",
    ),
    attr(
        "overflow-x",
        ElementAttributeType::String,
        "Horizontal overflow behavior",
    ),
    attr(
        "overflow-y",
        ElementAttributeType::String,
        "Vertical overflow behavior",
    ),
    attr(
        "scroll-x",
        ElementAttributeType::Number,
        "Initial horizontal scroll offset",
    ),
    attr(
        "scroll-y",
        ElementAttributeType::Number,
        "Initial vertical scroll offset",
    ),
    attr(
        "columns",
        ElementAttributeType::String,
        "Conservative grid column track list",
    ),
    attr(
        "rows",
        ElementAttributeType::String,
        "Conservative grid row track list",
    ),
    attr(
        "column",
        ElementAttributeType::Number,
        "Grid column placement",
    ),
    attr("row", ElementAttributeType::Number, "Grid row placement"),
    attr(
        "column-span",
        ElementAttributeType::Number,
        "Grid column span",
    ),
    attr("row-span", ElementAttributeType::Number, "Grid row span"),
    attr("layer", ElementAttributeType::Number, "Stacking layer"),
    attr("for", ElementAttributeType::String, "Associated element id"),
    attr("src", ElementAttributeType::String, "Image or icon source"),
    attr(
        "name",
        ElementAttributeType::String,
        "Icon or shortcut name",
    ),
    attr(
        "alt",
        ElementAttributeType::String,
        "Accessible alternate text",
    ),
    attr("size", ElementAttributeType::Number, "Display size hint"),
    attr("key", ElementAttributeType::String, "Shortcut key label"),
    attr("tooltip", ElementAttributeType::String, "Tooltip text"),
    attr(
        "tooltip-for",
        ElementAttributeType::String,
        "Tooltip owner element id",
    ),
    attr(
        "indeterminate",
        ElementAttributeType::Boolean,
        "Progress has no determinate value",
    ),
];

static COMMON_STATES: &[ElementStateFlag] = &[
    ElementStateFlag::Disabled,
    ElementStateFlag::ReadOnly,
    ElementStateFlag::Required,
    ElementStateFlag::Focused,
    ElementStateFlag::Selected,
    ElementStateFlag::Checked,
    ElementStateFlag::Expanded,
    ElementStateFlag::Pressed,
    ElementStateFlag::Invalid,
    ElementStateFlag::Active,
    ElementStateFlag::Value,
];

static COMMON_EVENTS: &[ElementEventDef] = &[
    event("click", "element", "Activation from pointer or keyboard"),
    event("input", "value", "Immediate value input"),
    event("change", "value", "Committed value change"),
    event("select", "value", "Selection change"),
    event("activate", "element", "Command or item activation"),
    event("openchange", "open", "Open state change"),
];

static COMMON_STYLE_HOOKS: &[&str] = &[
    "disabled",
    "busy",
    "default",
    "destructive",
    "readonly",
    "required",
    "focus",
    "focus-visible",
    "selected",
    "checked",
    "expanded",
    "pressed",
    "invalid",
    "active",
    "value",
    "layout",
    "display",
    "structure",
    "progress",
    "tooltip",
];

static HTML_REF: &[ElementCompatibilityRef] = &[compat(
    "HTML",
    "form and semantic elements",
    "Coverage inspiration only; MESH does not implement browser form semantics.",
)];
static QT_REF: &[ElementCompatibilityRef] = &[compat(
    "Qt Widgets/layouts",
    "controls and layout categories",
    "Coverage inspiration only; MESH owns retained rendering and diagnostics.",
)];
static FLUTTER_REF: &[ElementCompatibilityRef] = &[compat(
    "Flutter",
    "widget categories",
    "Coverage inspiration only; MESH markup is not a Flutter compatibility layer.",
)];

macro_rules! contract {
    ($kind:ident, $tag:literal, $family:ident, $role:expr, $focusable:expr, $compat:expr) => {
        ElementContractDef {
            kind: ElementKind::$kind,
            tag: $tag,
            family: ElementFamily::$family,
            type_name: ElementKind::$kind.type_name(),
            attributes: COMMON_ATTRIBUTES,
            states: COMMON_STATES,
            events: COMMON_EVENTS,
            accessibility: ElementAccessibilityDef {
                role: $role,
                focusable: $focusable,
                label_required: $focusable,
            },
            style_hooks: COMMON_STYLE_HOOKS,
            compatibility: $compat,
        }
    };
}

pub static ELEMENT_CONTRACT_DEFS: &[ElementContractDef] = &[
    contract!(
        Box,
        "box",
        Layout,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(Row, "row", Layout, AccessibilityRole::Region, false, QT_REF),
    contract!(
        Column,
        "column",
        Layout,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Grid,
        "grid",
        Layout,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Stack,
        "stack",
        Layout,
        AccessibilityRole::Region,
        false,
        FLUTTER_REF
    ),
    contract!(
        Spacer,
        "spacer",
        Layout,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Divider,
        "divider",
        Layout,
        AccessibilityRole::Separator,
        false,
        HTML_REF
    ),
    contract!(
        Separator,
        "separator",
        Layout,
        AccessibilityRole::Separator,
        false,
        QT_REF
    ),
    contract!(
        ScrollArea,
        "scroll-area",
        Layout,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Section,
        "section",
        Layout,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        Header,
        "header",
        Layout,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        Footer,
        "footer",
        Layout,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        Group,
        "group",
        Layout,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        FormRow,
        "form-row",
        Layout,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Text,
        "text",
        Display,
        AccessibilityRole::Label,
        false,
        HTML_REF
    ),
    contract!(
        Icon,
        "icon",
        Display,
        AccessibilityRole::Image,
        false,
        HTML_REF
    ),
    contract!(
        Image,
        "image",
        Display,
        AccessibilityRole::Image,
        false,
        HTML_REF
    ),
    contract!(
        Badge,
        "badge",
        Display,
        AccessibilityRole::Status,
        false,
        FLUTTER_REF
    ),
    contract!(
        Progress,
        "progress",
        Display,
        AccessibilityRole::ProgressBar,
        false,
        HTML_REF
    ),
    contract!(
        Meter,
        "meter",
        Display,
        AccessibilityRole::ProgressBar,
        false,
        HTML_REF
    ),
    contract!(
        Tooltip,
        "tooltip",
        Display,
        AccessibilityRole::Alert,
        false,
        HTML_REF
    ),
    contract!(
        Avatar,
        "avatar",
        Display,
        AccessibilityRole::Image,
        false,
        FLUTTER_REF
    ),
    contract!(
        Shortcut,
        "shortcut",
        Display,
        AccessibilityRole::Label,
        false,
        QT_REF
    ),
    contract!(
        Button,
        "button",
        Action,
        AccessibilityRole::Button,
        true,
        HTML_REF
    ),
    contract!(
        IconButton,
        "icon-button",
        Action,
        AccessibilityRole::Button,
        true,
        FLUTTER_REF
    ),
    contract!(
        ToggleButton,
        "toggle-button",
        Action,
        AccessibilityRole::Button,
        true,
        QT_REF
    ),
    contract!(
        CommandButton,
        "command-button",
        Action,
        AccessibilityRole::Button,
        true,
        QT_REF
    ),
    contract!(
        LinkButton,
        "link-button",
        Action,
        AccessibilityRole::Button,
        true,
        HTML_REF
    ),
    contract!(
        Input,
        "input",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        HTML_REF
    ),
    contract!(
        TextArea,
        "textarea",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        HTML_REF
    ),
    contract!(
        Search,
        "search",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        HTML_REF
    ),
    contract!(
        Password,
        "password",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        HTML_REF
    ),
    contract!(
        NumberInput,
        "number-input",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        HTML_REF
    ),
    contract!(
        Stepper,
        "stepper",
        TextInput,
        AccessibilityRole::TextInput,
        true,
        QT_REF
    ),
    contract!(
        Select,
        "select",
        ChoiceMenu,
        AccessibilityRole::Menu,
        true,
        HTML_REF
    ),
    contract!(
        Option,
        "option",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        false,
        HTML_REF
    ),
    contract!(
        Checkbox,
        "checkbox",
        ChoiceMenu,
        AccessibilityRole::Checkbox,
        true,
        HTML_REF
    ),
    contract!(
        Switch,
        "switch",
        ChoiceMenu,
        AccessibilityRole::Switch,
        true,
        FLUTTER_REF
    ),
    contract!(
        Radio,
        "radio",
        ChoiceMenu,
        AccessibilityRole::Checkbox,
        true,
        HTML_REF
    ),
    contract!(
        RadioGroup,
        "radio-group",
        ChoiceMenu,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        SegmentedControl,
        "segmented-control",
        ChoiceMenu,
        AccessibilityRole::Toolbar,
        true,
        QT_REF
    ),
    contract!(
        Menu,
        "menu",
        ChoiceMenu,
        AccessibilityRole::Menu,
        true,
        QT_REF
    ),
    contract!(
        MenuItem,
        "menu-item",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        true,
        QT_REF
    ),
    contract!(
        CommandItem,
        "command-item",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        true,
        QT_REF
    ),
    contract!(
        PreferenceRow,
        "preference-row",
        ChoiceMenu,
        AccessibilityRole::Region,
        true,
        QT_REF
    ),
    contract!(
        Panel,
        "panel",
        Container,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Popover,
        "popover",
        Container,
        AccessibilityRole::Dialog,
        true,
        FLUTTER_REF
    ),
    contract!(
        Dialog,
        "dialog",
        Container,
        AccessibilityRole::Dialog,
        true,
        HTML_REF
    ),
    contract!(
        Sheet,
        "sheet",
        Container,
        AccessibilityRole::Dialog,
        true,
        FLUTTER_REF
    ),
    contract!(
        Tabs,
        "tabs",
        Container,
        AccessibilityRole::Toolbar,
        true,
        HTML_REF
    ),
    contract!(
        Tab,
        "tab",
        Container,
        AccessibilityRole::Tab,
        true,
        HTML_REF
    ),
    contract!(
        Accordion,
        "accordion",
        Container,
        AccessibilityRole::Region,
        true,
        HTML_REF
    ),
    contract!(
        Details,
        "details",
        Container,
        AccessibilityRole::Region,
        true,
        HTML_REF
    ),
    contract!(
        List,
        "list",
        Collection,
        AccessibilityRole::List,
        true,
        HTML_REF
    ),
    contract!(
        ListItem,
        "list-item",
        Collection,
        AccessibilityRole::ListItem,
        true,
        HTML_REF
    ),
    contract!(
        Table,
        "table",
        Collection,
        AccessibilityRole::Region,
        true,
        HTML_REF
    ),
    contract!(
        Cell,
        "cell",
        Collection,
        AccessibilityRole::Region,
        false,
        HTML_REF
    ),
    contract!(
        Tree,
        "tree",
        Collection,
        AccessibilityRole::Region,
        true,
        QT_REF
    ),
    contract!(
        EmptyState,
        "empty-state",
        Collection,
        AccessibilityRole::Status,
        false,
        FLUTTER_REF
    ),
    contract!(
        Slot,
        "slot",
        Shell,
        AccessibilityRole::Region,
        false,
        FLUTTER_REF
    ),
    contract!(
        Surface,
        "surface",
        Shell,
        AccessibilityRole::Region,
        false,
        QT_REF
    ),
    contract!(
        Widget,
        "widget",
        Shell,
        AccessibilityRole::Region,
        false,
        FLUTTER_REF
    ),
];

pub static ELEMENT_TYPE_DEFS: &[ElementTypeDef] = &[
    element_type(ElementKind::Box, "box", "MeshElement", &[]),
    element_type(ElementKind::Row, "row", "RowElement", &[]),
    element_type(ElementKind::Column, "column", "ColumnElement", &[]),
    element_type(ElementKind::Stack, "stack", "StackElement", &[]),
    element_type(ElementKind::Scroll, "scroll", "ScrollElement", &[]),
    element_type(ElementKind::ScrollView, "scroll-view", "ScrollElement", &[]),
    element_type(ElementKind::Spacer, "spacer", "SpacerElement", &[]),
    element_type(ElementKind::Separator, "separator", "SeparatorElement", &[]),
    element_type(ElementKind::Text, "text", "TextElement", TEXT_FIELDS),
    element_type(ElementKind::Label, "label", "LabelElement", LABEL_FIELDS),
    element_type(ElementKind::Icon, "icon", "IconElement", ICON_FIELDS),
    element_type(ElementKind::Image, "image", "ImageElement", IMAGE_FIELDS),
    element_type(
        ElementKind::Button,
        "button",
        "ButtonElement",
        BUTTON_FIELDS,
    ),
    element_type(
        ElementKind::IconButton,
        "icon-button",
        "IconButtonElement",
        ICON_FIELDS,
    ),
    element_type(ElementKind::Input, "input", "InputElement", INPUT_FIELDS),
    element_type(
        ElementKind::Slider,
        "slider",
        "SliderElement",
        SLIDER_FIELDS,
    ),
    element_type(
        ElementKind::Switch,
        "switch",
        "SwitchElement",
        CHECKABLE_FIELDS,
    ),
    element_type(
        ElementKind::Checkbox,
        "checkbox",
        "CheckboxElement",
        CHECKABLE_FIELDS,
    ),
    element_type(ElementKind::List, "list", "ListElement", &[]),
    element_type(ElementKind::ListItem, "list-item", "ListItemElement", &[]),
    element_type(ElementKind::Slot, "slot", "SlotElement", &[]),
    element_type(ElementKind::Surface, "surface", "SurfaceElement", &[]),
    element_type(ElementKind::Widget, "widget", "WidgetElement", &[]),
];

const fn field(
    name: &'static str,
    field_type: ElementFieldType,
    optional: bool,
    description: &'static str,
) -> ElementFieldDef {
    ElementFieldDef {
        name,
        field_type,
        optional,
        description,
    }
}

const fn attr(
    name: &'static str,
    attribute_type: ElementAttributeType,
    description: &'static str,
) -> ElementAttributeDef {
    ElementAttributeDef {
        name,
        attribute_type,
        description,
    }
}

const fn event(
    name: &'static str,
    payload: &'static str,
    description: &'static str,
) -> ElementEventDef {
    ElementEventDef {
        name,
        payload,
        description,
    }
}

const fn compat(
    source: &'static str,
    reference: &'static str,
    note: &'static str,
) -> ElementCompatibilityRef {
    ElementCompatibilityRef {
        source,
        reference,
        note,
    }
}

const fn element_type(
    kind: ElementKind,
    tag: &'static str,
    type_name: &'static str,
    fields: &'static [ElementFieldDef],
) -> ElementTypeDef {
    ElementTypeDef {
        kind,
        tag,
        type_name,
        fields,
    }
}

pub fn element_type_for_tag(tag: &str) -> &'static ElementTypeDef {
    ELEMENT_TYPE_DEFS
        .iter()
        .find(|def| def.tag == tag)
        .unwrap_or(&ELEMENT_TYPE_DEFS[0])
}

pub fn element_contract_for_tag(tag: &str) -> Option<&'static ElementContractDef> {
    ELEMENT_CONTRACT_DEFS.iter().find(|def| def.tag == tag)
}

pub fn element_contract_tags() -> impl Iterator<Item = &'static str> {
    ELEMENT_CONTRACT_DEFS.iter().map(|def| def.tag)
}

pub fn common_state_flags() -> &'static [ElementStateFlag] {
    COMMON_STATES
}

pub fn validate_element_attribute(tag: &str, name: &str, value: &str) -> Option<ElementDiagnostic> {
    let contract = element_contract_for_tag(tag)?;
    if let Some(diagnostic) = validate_phase87_attribute_value(tag, name, value) {
        return Some(diagnostic);
    }
    if contract.attributes.iter().any(|attr| attr.name == name)
        || name.starts_with("data-")
        || name.starts_with("aria-")
        || name.starts_with("bind:")
        || name.starts_with("on")
    {
        return None;
    }

    Some(ElementDiagnostic {
        tag: tag.to_string(),
        name: name.to_string(),
        kind: ElementDiagnosticKind::UnsupportedAttribute,
        message: format!("unsupported attribute '{name}' on <{tag}>"),
        action: format!(
            "Remove the attribute or use one of: {}",
            contract
                .attributes
                .iter()
                .map(|attr| attr.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

fn validate_phase87_attribute_value(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    match (tag, name) {
        ("grid", "columns" | "rows") => validate_grid_tracks(tag, name, value),
        ("progress", "min" | "max" | "value") => validate_number_attribute(tag, name, value),
        ("progress", "indeterminate") => validate_bool_attribute(tag, name, value),
        ("button", "icon" | "name" | "src") => Some(invalid_attr(
            tag,
            name,
            "buttons do not accept icon shortcut attributes",
            "Put a dedicated <icon> element inside <button> markup instead.",
        )),
        ("button", "busy" | "default" | "destructive" | "pressed" | "disabled") => {
            validate_bool_attribute(tag, name, value)
        }
        (
            "input" | "textarea" | "search" | "password" | "number-input" | "stepper",
            "disabled" | "readonly" | "required" | "invalid" | "multiline" | "masked",
        ) => validate_bool_attribute(tag, name, value),
        (
            "select" | "option" | "checkbox" | "switch" | "radio" | "radio-group" | "menu"
            | "menu-item" | "command-item" | "preference-row",
            "disabled" | "checked" | "selected" | "expanded" | "required" | "invalid",
        ) => validate_bool_attribute(tag, name, value),
        (
            "popover" | "dialog" | "sheet" | "tabs" | "tab" | "accordion" | "details" | "list"
            | "list-item" | "table" | "cell" | "tree" | "empty-state",
            "open" | "expanded" | "selected" | "active" | "disabled" | "hidden",
        ) => validate_bool_attribute(tag, name, value),
        ("dialog" | "popover", "label" | "aria-label") if value.trim().is_empty() => {
            Some(invalid_attr(
                tag,
                name,
                "interactive containers need a non-empty accessible label",
                "Provide visible text, label, or aria-label for the container.",
            ))
        }
        ("option", "value") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "options need a non-empty value",
            "Set value to the string that the parent <select> should receive on change.",
        )),
        ("radio", "value") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "radio choices need a non-empty value",
            "Set value to the string that the parent <radio-group> should receive on change.",
        )),
        ("radio", "name") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "radio choices need group metadata when not nested in a radio-group",
            "Wrap radios in <radio-group> or set a non-empty name.",
        )),
        ("number-input" | "stepper", "min" | "max" | "value") => {
            validate_number_attribute(tag, name, value)
        }
        ("number-input" | "stepper", "step") => {
            validate_positive_number_attribute(tag, name, value)
        }
        ("button" | "command-button" | "link-button", "form" | "action" | "method") => {
            Some(invalid_attr(
                tag,
                name,
                "browser form behavior is not supported by MESH buttons",
                "Use a Luau handler such as onclick or onactivate.",
            ))
        }
        ("tooltip", "tooltip-for") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "tooltip-for must reference a non-empty owner id",
            "Set tooltip-for to the id of the element that owns this tooltip.",
        )),
        ("section" | "header" | "footer" | "group" | "form-row", "value" | "checked") => {
            Some(invalid_attr(
                tag,
                name,
                "structure elements do not expose control value state",
                "Move value state to an input/control element or remove the attribute.",
            ))
        }
        _ => None,
    }
}

fn validate_grid_tracks(tag: &str, name: &str, value: &str) -> Option<ElementDiagnostic> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(invalid_attr(
            tag,
            name,
            "grid tracks cannot be empty",
            "Use a space-separated list of fixed pixel values or auto tracks.",
        ));
    }

    for track in trimmed.split_whitespace() {
        if track == "auto" {
            continue;
        }
        if let Some(px) = track.strip_suffix("px")
            && px.parse::<f32>().is_ok_and(|value| value >= 0.0)
        {
            continue;
        }
        return Some(invalid_attr(
            tag,
            name,
            "unsupported grid track value",
            "Use only fixed pixel tracks like 120px or auto in Phase 87.",
        ));
    }

    None
}

fn validate_number_attribute(tag: &str, name: &str, value: &str) -> Option<ElementDiagnostic> {
    if value.trim().is_empty() || value.trim().parse::<f32>().is_ok() {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a numeric value",
        "Use a numeric literal or a binding that resolves to a number.",
    ))
}

fn validate_positive_number_attribute(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    if value.trim().is_empty() || value.trim().parse::<f32>().is_ok_and(|parsed| parsed > 0.0) {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a positive numeric value",
        "Use a positive numeric literal or a binding that resolves to one.",
    ))
}

fn validate_bool_attribute(tag: &str, name: &str, value: &str) -> Option<ElementDiagnostic> {
    if matches!(value.trim(), "" | "true" | "false") {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a boolean value",
        "Use true, false, or omit the value for true.",
    ))
}

fn invalid_attr(tag: &str, name: &str, message: &str, action: &str) -> ElementDiagnostic {
    ElementDiagnostic {
        tag: tag.to_string(),
        name: name.to_string(),
        kind: ElementDiagnosticKind::InvalidAttributeValue,
        message: format!("invalid attribute '{name}' on <{tag}>: {message}"),
        action: action.to_string(),
    }
}

pub fn validate_element_event(tag: &str, event_name: &str) -> Option<ElementDiagnostic> {
    let contract = element_contract_for_tag(tag)?;
    if contract.events.iter().any(|event| event.name == event_name) {
        return None;
    }

    Some(ElementDiagnostic {
        tag: tag.to_string(),
        name: event_name.to_string(),
        kind: ElementDiagnosticKind::UnsupportedEvent,
        message: format!("unsupported event '{event_name}' on <{tag}>"),
        action: format!(
            "Remove the handler or use one of: {}",
            contract
                .events
                .iter()
                .map(|event| event.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ElementStateSnapshot {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub required: bool,
    pub selected: bool,
    pub checked: bool,
    pub expanded: bool,
    pub pressed: bool,
    pub invalid: bool,
    pub value: bool,
}

impl From<ElementState> for ElementStateSnapshot {
    fn from(state: ElementState) -> Self {
        Self {
            hovered: state.hovered,
            active: state.active,
            focused: state.focused,
            disabled: state.disabled,
            read_only: state.read_only,
            required: state.required,
            selected: state.selected,
            checked: state.checked,
            expanded: state.expanded,
            pressed: state.pressed,
            invalid: state.invalid,
            value: state.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementSnapshot {
    pub key: String,
    pub id: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub tag: String,
    pub element_type: String,
    pub x: f32,
    pub y: f32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub width: f32,
    pub height: f32,
    pub client_left: f32,
    pub client_top: f32,
    pub client_width: f32,
    pub client_height: f32,
    #[serde(rename = "clientBoundRect")]
    pub client_bound_rect_camel: ElementRect,
    pub client_bound_rect: ElementRect,
    pub bounding_client_rect: ElementRect,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub scroll_left: f32,
    pub scroll_top: f32,
    pub scroll_width: f32,
    pub scroll_height: f32,
    pub max_scroll_left: f32,
    pub max_scroll_top: f32,
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub required: bool,
    pub selected: bool,
    pub checked: bool,
    pub expanded: bool,
    pub pressed: bool,
    pub invalid: bool,
    pub value: bool,
    pub attributes: BTreeMap<String, String>,
}

pub fn element_snapshot(node: &WidgetNode, offset_x: f32, offset_y: f32) -> ElementSnapshot {
    let left = node.layout.x + offset_x;
    let top = node.layout.y + offset_y;
    let width = node.layout.width.max(0.0);
    let height = node.layout.height.max(0.0);
    let right = left + width;
    let bottom = top + height;
    let client_left = left + node.computed_style.padding.left;
    let client_top = top + node.computed_style.padding.top;
    let client_width = (width - node.computed_style.padding.horizontal()).max(0.0);
    let client_height = (height - node.computed_style.padding.vertical()).max(0.0);
    let client_right = client_left + client_width;
    let client_bottom = client_top + client_height;
    let scroll_x = node
        .attributes
        .get("_mesh_scroll_x")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    let scroll_y = node
        .attributes
        .get("_mesh_scroll_y")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    let attr_f32 = |key: &str| {
        node.attributes
            .get(key)
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    let max_scroll_left = attr_f32("_mesh_scroll_max_x");
    let max_scroll_top = attr_f32("_mesh_scroll_max_y");
    // Full content extent = viewport content box + the overflow we can scroll to.
    let scroll_width = client_width + max_scroll_left;
    let scroll_height = client_height + max_scroll_top;
    let state = ElementStateSnapshot::from(node.state);
    let element_type = element_type_for_tag(&node.tag).type_name.to_string();
    let client_bound_rect = ElementRect {
        left: client_left,
        top: client_top,
        right: client_right,
        bottom: client_bottom,
        width: client_width,
        height: client_height,
    };
    let bounding_client_rect = ElementRect {
        left,
        top,
        right,
        bottom,
        width,
        height,
    };

    ElementSnapshot {
        key: node
            .attributes
            .get("_mesh_key")
            .cloned()
            .unwrap_or_default(),
        id: node.attributes.get("id").cloned(),
        reference: node.attributes.get("ref").cloned(),
        tag: node.tag.clone(),
        element_type,
        x: left,
        y: top,
        left,
        top,
        right,
        bottom,
        width,
        height,
        client_left,
        client_top,
        client_width,
        client_height,
        client_bound_rect_camel: client_bound_rect.clone(),
        client_bound_rect,
        bounding_client_rect,
        scroll_x,
        scroll_y,
        scroll_left: scroll_x,
        scroll_top: scroll_y,
        scroll_width,
        scroll_height,
        max_scroll_left,
        max_scroll_top,
        hovered: state.hovered,
        active: state.active,
        focused: state.focused,
        disabled: state.disabled,
        read_only: state.read_only,
        required: state.required,
        selected: state.selected,
        checked: state.checked,
        expanded: state.expanded,
        pressed: state.pressed,
        invalid: state.invalid,
        value: state.value,
        attributes: node
            .attributes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

pub fn element_snapshot_json(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Value {
    let snapshot = element_snapshot(node, offset_x, offset_y);
    let mut value = serde_json::to_value(snapshot).unwrap_or_else(|_| json!({}));
    let Some(object) = value.as_object_mut() else {
        return value;
    };

    expose_tag_specific_fields(object, node);
    value
}

fn expose_tag_specific_fields(object: &mut Map<String, Value>, node: &WidgetNode) {
    let def = element_type_for_tag(&node.tag);
    for field in def.fields {
        let Some(raw) = node.attributes.get(field.name) else {
            continue;
        };
        object.insert(
            field.name.to_string(),
            coerce_field_value(raw, field.field_type),
        );
    }
}

fn coerce_field_value(raw: &str, field_type: ElementFieldType) -> Value {
    match field_type {
        ElementFieldType::Number => raw
            .parse::<f64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(raw.to_string())),
        ElementFieldType::Boolean => match raw {
            "true" | "" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(raw.to_string()),
        },
        ElementFieldType::String | ElementFieldType::Rect | ElementFieldType::Object => {
            Value::String(raw.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dimension, Edges};

    #[test]
    fn icon_snapshot_exposes_base_and_icon_fields() {
        let mut node = WidgetNode::new("icon");
        node.attributes.insert("_mesh_key".into(), "0:1".into());
        node.attributes.insert("ref".into(), "batteryIcon".into());
        node.attributes.insert("name".into(), "battery-full".into());
        node.attributes.insert("size".into(), "18".into());
        node.layout.x = 10.0;
        node.layout.y = 20.0;
        node.layout.width = 24.0;
        node.layout.height = 24.0;
        node.computed_style.padding = Edges::all(2.0);

        let value = element_snapshot_json(&node, 0.0, 0.0);

        assert_eq!(value["element_type"], "IconElement");
        assert_eq!(value["ref"], "batteryIcon");
        assert_eq!(value["name"], "battery-full");
        assert_eq!(value["size"], 18.0);
        assert_eq!(value["width"], 24.0);
        assert_eq!(value["client_width"], 20.0);
    }

    #[test]
    fn input_type_def_is_lookupable_by_tag() {
        let def = element_type_for_tag("input");

        assert_eq!(def.type_name, "InputElement");
        assert!(def.fields.iter().any(|field| field.name == "value"));
    }

    #[test]
    fn input_snapshot_keeps_input_type_separate_from_element_type() {
        let mut node = WidgetNode::new("input");
        node.attributes.insert("ref".into(), "searchBox".into());
        node.attributes.insert("type".into(), "search".into());
        node.attributes.insert("value".into(), "mesh".into());

        let value = element_snapshot_json(&node, 0.0, 0.0);

        assert_eq!(value["element_type"], "InputElement");
        assert_eq!(value["type"], "search");
        assert_eq!(value["value"], "mesh");
    }

    #[test]
    fn unknown_tags_fall_back_to_mesh_element() {
        let mut node = WidgetNode::new("custom");
        node.computed_style.width = Dimension::Auto;

        let value = element_snapshot_json(&node, 0.0, 0.0);

        assert_eq!(value["element_type"], "MeshElement");
    }

    #[test]
    fn element_contract_metadata_types_are_available() {
        let contract = element_contract_for_tag("button").expect("button contract");

        assert_eq!(contract.family, ElementFamily::Action);
        assert_eq!(contract.accessibility.role, AccessibilityRole::Button);
        assert!(
            contract
                .attributes
                .iter()
                .any(|attribute| attribute.name == "disabled")
        );
        assert!(contract.events.iter().any(|event| event.name == "change"));
    }

    #[test]
    fn element_contract_covers_v1_16_taxonomy() {
        let required = [
            "grid",
            "scroll-area",
            "form-row",
            "badge",
            "progress",
            "tooltip",
            "toggle-button",
            "textarea",
            "number-input",
            "select",
            "radio-group",
            "segmented-control",
            "menu",
            "command-item",
            "popover",
            "tabs",
            "table",
            "tree",
            "empty-state",
            "surface",
        ];

        for tag in required {
            assert!(
                element_contract_for_tag(tag).is_some(),
                "missing contract for {tag}"
            );
        }

        let families: std::collections::BTreeSet<_> = ELEMENT_CONTRACT_DEFS
            .iter()
            .map(|contract| contract.family)
            .collect();
        assert!(families.contains(&ElementFamily::Layout));
        assert!(families.contains(&ElementFamily::Display));
        assert!(families.contains(&ElementFamily::Action));
        assert!(families.contains(&ElementFamily::TextInput));
        assert!(families.contains(&ElementFamily::ChoiceMenu));
        assert!(families.contains(&ElementFamily::Container));
        assert!(families.contains(&ElementFamily::Collection));
        assert!(families.contains(&ElementFamily::Shell));
    }

    #[test]
    fn element_state_snapshot_exposes_shared_control_state() {
        let state = ElementState {
            hovered: true,
            active: true,
            focused: true,
            focus_visible: true,
            disabled: true,
            read_only: true,
            required: true,
            selected: true,
            checked: true,
            expanded: true,
            pressed: true,
            invalid: true,
            value: true,
        };

        let snapshot = ElementStateSnapshot::from(state);

        assert!(snapshot.read_only);
        assert!(snapshot.required);
        assert!(snapshot.selected);
        assert!(snapshot.expanded);
        assert!(snapshot.pressed);
        assert!(snapshot.invalid);
        assert!(snapshot.value);
    }

    #[test]
    fn element_contract_common_state_flags_cover_required_set() {
        let flags = common_state_flags();

        for flag in [
            ElementStateFlag::Disabled,
            ElementStateFlag::ReadOnly,
            ElementStateFlag::Required,
            ElementStateFlag::Focused,
            ElementStateFlag::Selected,
            ElementStateFlag::Checked,
            ElementStateFlag::Expanded,
            ElementStateFlag::Pressed,
            ElementStateFlag::Invalid,
            ElementStateFlag::Active,
            ElementStateFlag::Value,
        ] {
            assert!(flags.contains(&flag), "missing {flag:?}");
        }
    }

    #[test]
    fn phase87_layout_display_contract_exposes_required_metadata() {
        let grid = element_contract_for_tag("grid").expect("grid contract");
        assert!(grid.attributes.iter().any(|attr| attr.name == "columns"));
        assert!(grid.attributes.iter().any(|attr| attr.name == "rows"));
        assert!(grid.attributes.iter().any(|attr| attr.name == "column"));
        assert!(grid.style_hooks.contains(&"layout"));

        let scroll_area = element_contract_for_tag("scroll-area").expect("scroll-area contract");
        assert!(
            scroll_area
                .attributes
                .iter()
                .any(|attr| attr.name == "overflow-y")
        );

        for tag in ["section", "header", "footer", "group", "form-row"] {
            let contract = element_contract_for_tag(tag).expect("structure contract");
            assert_eq!(contract.family, ElementFamily::Layout);
            assert!(contract.attributes.iter().any(|attr| attr.name == "label"));
            assert!(contract.style_hooks.contains(&"structure"));
        }

        let progress = element_contract_for_tag("progress").expect("progress contract");
        assert_eq!(progress.accessibility.role, AccessibilityRole::ProgressBar);
        assert!(progress.attributes.iter().any(|attr| attr.name == "min"));
        assert!(progress.attributes.iter().any(|attr| attr.name == "max"));
        assert!(
            progress
                .attributes
                .iter()
                .any(|attr| attr.name == "indeterminate")
        );
        assert!(progress.style_hooks.contains(&"progress"));

        let meter = element_contract_for_tag("meter").expect("meter contract");
        assert_eq!(meter.family, ElementFamily::Display);
        assert_eq!(meter.accessibility.role, AccessibilityRole::ProgressBar);

        for tag in ["badge", "avatar", "shortcut", "tooltip"] {
            let contract = element_contract_for_tag(tag).expect("display contract");
            assert_eq!(contract.family, ElementFamily::Display);
            assert!(contract.style_hooks.contains(&"display"));
        }
    }

    #[test]
    fn phase87_layout_display_diagnostics_validate_values() {
        let diagnostic = validate_element_attribute("grid", "columns", "1fr 2fr")
            .expect("invalid grid track diagnostic");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
        assert!(diagnostic.message.contains("unsupported grid track"));

        assert!(validate_element_attribute("grid", "columns", "120px auto").is_none());

        let diagnostic = validate_element_attribute("progress", "value", "half")
            .expect("invalid progress value diagnostic");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
        assert!(diagnostic.message.contains("expected a numeric value"));

        let diagnostic = validate_element_attribute("progress", "indeterminate", "maybe")
            .expect("invalid progress boolean diagnostic");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );

        let diagnostic = validate_element_attribute("tooltip", "tooltip-for", "")
            .expect("invalid tooltip owner diagnostic");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );

        let diagnostic = validate_element_attribute("section", "value", "active")
            .expect("structure value diagnostic");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
    }

    #[test]
    fn phase88_single_button_contract_rejects_icon_shortcut_attributes() {
        let button = element_contract_for_tag("button").expect("button contract");

        assert_eq!(button.family, ElementFamily::Action);
        assert_eq!(button.accessibility.role, AccessibilityRole::Button);
        for attr in ["pressed", "busy", "default", "destructive", "keybind"] {
            assert!(
                button
                    .attributes
                    .iter()
                    .any(|candidate| candidate.name == attr),
                "button should expose {attr}"
            );
        }

        for attr in ["icon", "name", "src"] {
            let diagnostic =
                validate_element_attribute("button", attr, "audio-volume-high").expect(attr);
            assert_eq!(
                diagnostic.kind,
                ElementDiagnosticKind::InvalidAttributeValue
            );
            assert!(diagnostic.action.contains("<icon>"));
        }
    }

    #[test]
    fn phase88_input_variant_contract_exposes_configured_input_metadata() {
        for tag in [
            "input",
            "textarea",
            "search",
            "password",
            "number-input",
            "stepper",
        ] {
            let contract = element_contract_for_tag(tag).expect("input contract");
            assert_eq!(contract.family, ElementFamily::TextInput);
            assert_eq!(contract.accessibility.role, AccessibilityRole::TextInput);
            for attr in [
                "value",
                "placeholder",
                "readonly",
                "required",
                "invalid",
                "type",
            ] {
                assert!(
                    contract
                        .attributes
                        .iter()
                        .any(|candidate| candidate.name == attr),
                    "{tag} should expose {attr}"
                );
            }
        }

        for attr in ["min", "max", "step"] {
            assert!(
                element_contract_for_tag("number-input")
                    .expect("number-input")
                    .attributes
                    .iter()
                    .any(|candidate| candidate.name == attr),
                "number-input should expose {attr}"
            );
        }
    }

    #[test]
    fn phase88_input_diagnostics_validate_numeric_and_boolean_values() {
        let diagnostic = validate_element_attribute("number-input", "value", "many")
            .expect("invalid numeric value");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
        assert!(diagnostic.message.contains("expected a numeric value"));

        let diagnostic =
            validate_element_attribute("stepper", "step", "0").expect("invalid step value");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
        assert!(diagnostic.message.contains("positive numeric"));

        let diagnostic =
            validate_element_attribute("textarea", "multiline", "sometimes").expect("bool value");
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );

        assert!(validate_element_attribute("number-input", "min", "0").is_none());
        assert!(validate_element_attribute("number-input", "max", "100").is_none());
        assert!(validate_element_attribute("number-input", "step", "5").is_none());
    }

    #[test]
    fn phase89_choice_and_menu_diagnostics_validate_authoring_state() {
        let option = validate_element_attribute("option", "value", "").expect("option diagnostic");
        assert_eq!(option.kind, ElementDiagnosticKind::InvalidAttributeValue);
        assert!(option.message.contains("options need"));

        let radio = validate_element_attribute("radio", "value", "").expect("radio diagnostic");
        assert_eq!(radio.kind, ElementDiagnosticKind::InvalidAttributeValue);
        assert!(radio.message.contains("radio choices"));

        let checked =
            validate_element_attribute("checkbox", "checked", "maybe").expect("bool diagnostic");
        assert_eq!(checked.kind, ElementDiagnosticKind::InvalidAttributeValue);

        assert!(validate_element_attribute("menu-item", "disabled", "true").is_none());
        assert!(validate_element_event("menu-item", "activate").is_none());
        assert!(validate_element_event("select", "change").is_none());
    }

    #[test]
    fn phase90_container_and_collection_diagnostics_validate_state() {
        let dialog =
            validate_element_attribute("dialog", "aria-label", "").expect("label diagnostic");
        assert_eq!(dialog.kind, ElementDiagnosticKind::InvalidAttributeValue);
        assert!(dialog.message.contains("accessible label"));

        let tab =
            validate_element_attribute("tab", "selected", "sometimes").expect("bool diagnostic");
        assert_eq!(tab.kind, ElementDiagnosticKind::InvalidAttributeValue);

        assert!(validate_element_attribute("details", "open", "true").is_none());
        assert!(validate_element_attribute("list-item", "selected", "false").is_none());
        assert!(validate_element_event("tab", "activate").is_none());
        assert!(validate_element_event("list-item", "activate").is_none());
    }

    #[test]
    fn element_diagnostic_unsupported_attribute_reports_author_action() {
        let diagnostic = validate_element_attribute("button", "browser-form-action", "submit")
            .expect("diagnostic");

        assert_eq!(diagnostic.kind, ElementDiagnosticKind::UnsupportedAttribute);
        assert_eq!(diagnostic.tag, "button");
        assert_eq!(diagnostic.name, "browser-form-action");
        assert!(
            diagnostic
                .action
                .contains("Remove the attribute or use one of")
        );
    }

    #[test]
    fn element_diagnostic_unsupported_event_reports_author_action() {
        let diagnostic = validate_element_event("button", "formsubmit").expect("diagnostic");

        assert_eq!(diagnostic.kind, ElementDiagnosticKind::UnsupportedEvent);
        assert_eq!(diagnostic.tag, "button");
        assert_eq!(diagnostic.name, "formsubmit");
        assert!(
            diagnostic
                .action
                .contains("Remove the handler or use one of")
        );
    }

    #[test]
    fn element_diagnostic_known_common_attribute_does_not_report_diagnostic() {
        assert!(validate_element_attribute("button", "disabled", "true").is_none());
    }
}
