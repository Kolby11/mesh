use super::*;

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
    pub attributes: crate::attributes::AttributeMap,
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
    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let max_scroll_left = scroll.max_x;
    let max_scroll_top = scroll.max_y;
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
        key: node.mesh_key().unwrap_or_default().to_owned(),
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
        attributes: node.attributes.clone(),
    }
}

pub fn element_snapshot_json(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Value {
    let mut object = element_snapshot_json_object(node, offset_x, offset_y);
    expose_tag_specific_fields(&mut object, node);
    Value::Object(object)
}

pub(super) fn element_snapshot_json_object(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Map<String, Value> {
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
    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let max_scroll_left = scroll.max_x;
    let max_scroll_top = scroll.max_y;
    let scroll_width = client_width + max_scroll_left;
    let scroll_height = client_height + max_scroll_top;
    let state = ElementStateSnapshot::from(node.state);

    let mut object = Map::with_capacity(45 + node.attributes.len());
    object.insert(
        "key".into(),
        Value::String(node.mesh_key().unwrap_or_default().to_owned()),
    );
    object.insert(
        "id".into(),
        node.attributes
            .get("id")
            .cloned()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "ref".into(),
        node.attributes
            .get("ref")
            .cloned()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert("tag".into(), Value::String(node.tag.clone()));
    object.insert(
        "element_type".into(),
        Value::String(element_type_for_tag(&node.tag).type_name.to_string()),
    );
    insert_f32(&mut object, "x", left);
    insert_f32(&mut object, "y", top);
    insert_f32(&mut object, "left", left);
    insert_f32(&mut object, "top", top);
    insert_f32(&mut object, "right", right);
    insert_f32(&mut object, "bottom", bottom);
    insert_f32(&mut object, "width", width);
    insert_f32(&mut object, "height", height);
    insert_f32(&mut object, "client_left", client_left);
    insert_f32(&mut object, "client_top", client_top);
    insert_f32(&mut object, "client_width", client_width);
    insert_f32(&mut object, "client_height", client_height);
    let client_bound_rect = element_rect_json(
        client_left,
        client_top,
        client_right,
        client_bottom,
        client_width,
        client_height,
    );
    object.insert("clientBoundRect".into(), client_bound_rect.clone());
    object.insert("client_bound_rect".into(), client_bound_rect);
    object.insert(
        "bounding_client_rect".into(),
        element_rect_json(left, top, right, bottom, width, height),
    );
    insert_f32(&mut object, "scroll_x", scroll_x);
    insert_f32(&mut object, "scroll_y", scroll_y);
    insert_f32(&mut object, "scroll_left", scroll_x);
    insert_f32(&mut object, "scroll_top", scroll_y);
    insert_f32(&mut object, "scroll_width", scroll_width);
    insert_f32(&mut object, "scroll_height", scroll_height);
    insert_f32(&mut object, "max_scroll_left", max_scroll_left);
    insert_f32(&mut object, "max_scroll_top", max_scroll_top);
    object.insert("hovered".into(), Value::Bool(state.hovered));
    object.insert("active".into(), Value::Bool(state.active));
    object.insert("focused".into(), Value::Bool(state.focused));
    object.insert("disabled".into(), Value::Bool(state.disabled));
    object.insert("read_only".into(), Value::Bool(state.read_only));
    object.insert("required".into(), Value::Bool(state.required));
    object.insert("selected".into(), Value::Bool(state.selected));
    object.insert("checked".into(), Value::Bool(state.checked));
    object.insert("expanded".into(), Value::Bool(state.expanded));
    object.insert("pressed".into(), Value::Bool(state.pressed));
    object.insert("invalid".into(), Value::Bool(state.invalid));
    object.insert("value".into(), Value::Bool(state.value));

    let mut attributes = Map::with_capacity(node.attributes.len());
    for (key, value) in &node.attributes {
        attributes.insert(key.as_str().to_string(), Value::String(value.clone()));
    }
    object.insert("attributes".into(), Value::Object(attributes));

    object
}

pub(super) fn element_rect_json(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    width: f32,
    height: f32,
) -> Value {
    let mut rect = Map::with_capacity(6);
    insert_f32(&mut rect, "left", left);
    insert_f32(&mut rect, "top", top);
    insert_f32(&mut rect, "right", right);
    insert_f32(&mut rect, "bottom", bottom);
    insert_f32(&mut rect, "width", width);
    insert_f32(&mut rect, "height", height);
    Value::Object(rect)
}

pub(super) fn insert_f32(object: &mut Map<String, Value>, key: &'static str, value: f32) {
    object.insert(key.into(), Value::from(value));
}

pub(super) fn expose_tag_specific_fields(object: &mut Map<String, Value>, node: &WidgetNode) {
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

pub(super) fn coerce_field_value(raw: &str, field_type: ElementFieldType) -> Value {
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
