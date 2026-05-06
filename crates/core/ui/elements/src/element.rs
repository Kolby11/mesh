//! Core element model exposed to runtime code and tooling.
//!
//! Elements are MESH-owned primitives (`button`, `icon`, `input`, etc.).
//! Components compose these primitives; modules package complete features.

use crate::{ElementState, WidgetNode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementKind {
    Box,
    Row,
    Column,
    Stack,
    Scroll,
    ScrollView,
    Spacer,
    Separator,
    Text,
    Label,
    Icon,
    Image,
    Button,
    IconButton,
    Input,
    Slider,
    Switch,
    Checkbox,
    List,
    ListItem,
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
            "stack" => Self::Stack,
            "scroll" => Self::Scroll,
            "scroll-view" => Self::ScrollView,
            "spacer" => Self::Spacer,
            "separator" => Self::Separator,
            "text" => Self::Text,
            "label" => Self::Label,
            "icon" => Self::Icon,
            "image" => Self::Image,
            "button" => Self::Button,
            "icon-button" => Self::IconButton,
            "input" => Self::Input,
            "slider" => Self::Slider,
            "switch" => Self::Switch,
            "checkbox" => Self::Checkbox,
            "list" => Self::List,
            "list-item" => Self::ListItem,
            "slot" => Self::Slot,
            "surface" => Self::Surface,
            "widget" => Self::Widget,
            _ => Self::Unknown,
        }
    }

    pub fn type_name(self) -> &'static str {
        match self {
            Self::Icon => "IconElement",
            Self::Image => "ImageElement",
            Self::Text => "TextElement",
            Self::Label => "LabelElement",
            Self::Button => "ButtonElement",
            Self::IconButton => "IconButtonElement",
            Self::Input => "InputElement",
            Self::Slider => "SliderElement",
            Self::Switch => "SwitchElement",
            Self::Checkbox => "CheckboxElement",
            Self::Row => "RowElement",
            Self::Column => "ColumnElement",
            Self::Stack => "StackElement",
            Self::Scroll | Self::ScrollView => "ScrollElement",
            Self::Spacer => "SpacerElement",
            Self::Separator => "SeparatorElement",
            Self::List => "ListElement",
            Self::ListItem => "ListItemElement",
            Self::Slot => "SlotElement",
            Self::Surface => "SurfaceElement",
            Self::Widget => "WidgetElement",
            Self::Box | Self::Unknown => "MeshElement",
        }
    }
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
    pub checked: bool,
}

impl From<ElementState> for ElementStateSnapshot {
    fn from(state: ElementState) -> Self {
        Self {
            hovered: state.hovered,
            active: state.active,
            focused: state.focused,
            disabled: state.disabled,
            checked: state.checked,
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
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
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
        hovered: state.hovered,
        active: state.active,
        focused: state.focused,
        disabled: state.disabled,
        checked: state.checked,
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
}
