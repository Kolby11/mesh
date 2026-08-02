//! Core element model exposed to runtime code and tooling.
//!
//! Elements are MESH-owned primitives (`button`, `icon`, `input`, etc.).
//! Components compose these primitives; modules package complete features.

use crate::{AccessibilityRole, ElementState, WidgetNode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod contracts;
mod snapshot;
mod validate;

pub use contracts::{BASE_ELEMENT_FIELDS, ELEMENT_CONTRACT_DEFS, ELEMENT_TYPE_DEFS};
pub use snapshot::{
    ElementRect, ElementSnapshot, ElementStateSnapshot, element_snapshot, element_snapshot_json,
};
pub use validate::{validate_element_attribute, validate_element_event};

use contracts::*;

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

pub fn element_type_for_tag(tag: &str) -> &'static ElementTypeDef {
    ELEMENT_TYPE_DEFS
        .iter()
        .find(|def| def.tag == tag)
        .unwrap_or(&ELEMENT_TYPE_DEFS[0])
}

pub fn element_contract_for_tag(tag: &str) -> Option<&'static ElementContractDef> {
    Some(&ELEMENT_CONTRACT_DEFS[contract_slot_for_tag(tag)?])
}

pub fn element_contract_tags() -> impl Iterator<Item = &'static str> {
    ELEMENT_CONTRACT_DEFS.iter().map(|def| def.tag)
}

pub fn common_state_flags() -> &'static [ElementStateFlag] {
    COMMON_STATES
}

#[cfg(test)]
mod tests;
