use super::*;

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

pub(super) static ICON_FIELDS: &[ElementFieldDef] = &[
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

pub(super) static TEXT_FIELDS: &[ElementFieldDef] = &[
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

pub(super) static BUTTON_FIELDS: &[ElementFieldDef] = &[
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
    field("variant", ElementFieldType::String, true, "Visual variant"),
];

pub(super) static INPUT_FIELDS: &[ElementFieldDef] = &[
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

pub(super) static SLIDER_FIELDS: &[ElementFieldDef] = &[
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

pub(super) static CHECKABLE_FIELDS: &[ElementFieldDef] = &[
    field("checked", ElementFieldType::Boolean, true, "Checked state"),
    field(
        "disabled",
        ElementFieldType::Boolean,
        true,
        "Disabled state",
    ),
];

pub(super) static IMAGE_FIELDS: &[ElementFieldDef] = &[
    field("src", ElementFieldType::String, true, "Image source path"),
    field(
        "alt",
        ElementFieldType::String,
        true,
        "Accessible alternate text",
    ),
];

pub(super) static LABEL_FIELDS: &[ElementFieldDef] = &[field(
    "for",
    ElementFieldType::String,
    true,
    "Associated input id",
)];

pub(super) static COMMON_ATTRIBUTES: &[ElementAttributeDef] = &[
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

pub(super) static COMMON_STATES: &[ElementStateFlag] = &[
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

pub(super) static COMMON_EVENTS: &[ElementEventDef] = &[
    event("click", "element", "Activation from pointer or keyboard"),
    event("input", "value", "Immediate value input"),
    event("change", "value", "Committed value change"),
    event("select", "value", "Selection change"),
    event("activate", "element", "Command or item activation"),
    event("openchange", "open", "Open state change"),
];

pub(super) static COMMON_STYLE_HOOKS: &[&str] = &[
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

macro_rules! contract {
    ($kind:ident, $tag:literal, $family:ident, $role:expr, $focusable:expr) => {
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
        }
    };
}

pub static ELEMENT_CONTRACT_DEFS: &[ElementContractDef] = &[
    contract!(Box, "box", Layout, AccessibilityRole::Region, false),
    contract!(Row, "row", Layout, AccessibilityRole::Region, false),
    contract!(Column, "column", Layout, AccessibilityRole::Region, false),
    contract!(Grid, "grid", Layout, AccessibilityRole::Region, false),
    contract!(Stack, "stack", Layout, AccessibilityRole::Region, false),
    contract!(Spacer, "spacer", Layout, AccessibilityRole::Region, false),
    contract!(
        Divider,
        "divider",
        Layout,
        AccessibilityRole::Separator,
        false
    ),
    contract!(
        Separator,
        "separator",
        Layout,
        AccessibilityRole::Separator,
        false
    ),
    contract!(
        ScrollArea,
        "scroll-area",
        Layout,
        AccessibilityRole::Region,
        false
    ),
    contract!(Section, "section", Layout, AccessibilityRole::Region, false),
    contract!(Header, "header", Layout, AccessibilityRole::Region, false),
    contract!(Footer, "footer", Layout, AccessibilityRole::Region, false),
    contract!(Group, "group", Layout, AccessibilityRole::Region, false),
    contract!(
        FormRow,
        "form-row",
        Layout,
        AccessibilityRole::Region,
        false
    ),
    contract!(Text, "text", Display, AccessibilityRole::Label, false),
    contract!(Icon, "icon", Display, AccessibilityRole::Image, false),
    contract!(Image, "image", Display, AccessibilityRole::Image, false),
    contract!(Badge, "badge", Display, AccessibilityRole::Status, false),
    contract!(
        Progress,
        "progress",
        Display,
        AccessibilityRole::ProgressBar,
        false
    ),
    contract!(
        Meter,
        "meter",
        Display,
        AccessibilityRole::ProgressBar,
        false
    ),
    contract!(Tooltip, "tooltip", Display, AccessibilityRole::Alert, false),
    contract!(Avatar, "avatar", Display, AccessibilityRole::Image, false),
    contract!(
        Shortcut,
        "shortcut",
        Display,
        AccessibilityRole::Label,
        false
    ),
    contract!(Button, "button", Action, AccessibilityRole::Button, true),
    contract!(
        IconButton,
        "icon-button",
        Action,
        AccessibilityRole::Button,
        true
    ),
    contract!(
        ToggleButton,
        "toggle-button",
        Action,
        AccessibilityRole::Button,
        true
    ),
    contract!(
        CommandButton,
        "command-button",
        Action,
        AccessibilityRole::Button,
        true
    ),
    contract!(
        LinkButton,
        "link-button",
        Action,
        AccessibilityRole::Button,
        true
    ),
    contract!(
        Input,
        "input",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(
        TextArea,
        "textarea",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(
        Search,
        "search",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(
        Password,
        "password",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(
        NumberInput,
        "number-input",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(
        Stepper,
        "stepper",
        TextInput,
        AccessibilityRole::TextInput,
        true
    ),
    contract!(Select, "select", ChoiceMenu, AccessibilityRole::Menu, true),
    contract!(
        Option,
        "option",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        false
    ),
    contract!(
        Checkbox,
        "checkbox",
        ChoiceMenu,
        AccessibilityRole::Checkbox,
        true
    ),
    contract!(
        Switch,
        "switch",
        ChoiceMenu,
        AccessibilityRole::Switch,
        true
    ),
    contract!(
        Radio,
        "radio",
        ChoiceMenu,
        AccessibilityRole::Checkbox,
        true
    ),
    contract!(
        RadioGroup,
        "radio-group",
        ChoiceMenu,
        AccessibilityRole::Region,
        false
    ),
    contract!(
        SegmentedControl,
        "segmented-control",
        ChoiceMenu,
        AccessibilityRole::Toolbar,
        true
    ),
    contract!(Menu, "menu", ChoiceMenu, AccessibilityRole::Menu, true),
    contract!(
        MenuItem,
        "menu-item",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        true
    ),
    contract!(
        CommandItem,
        "command-item",
        ChoiceMenu,
        AccessibilityRole::MenuItem,
        true
    ),
    contract!(
        PreferenceRow,
        "preference-row",
        ChoiceMenu,
        AccessibilityRole::Region,
        true
    ),
    contract!(Panel, "panel", Container, AccessibilityRole::Region, false),
    contract!(
        Popover,
        "popover",
        Container,
        AccessibilityRole::Dialog,
        true
    ),
    contract!(Dialog, "dialog", Container, AccessibilityRole::Dialog, true),
    contract!(Sheet, "sheet", Container, AccessibilityRole::Dialog, true),
    contract!(Tabs, "tabs", Container, AccessibilityRole::Toolbar, true),
    contract!(Tab, "tab", Container, AccessibilityRole::Tab, true),
    contract!(
        Accordion,
        "accordion",
        Container,
        AccessibilityRole::Region,
        true
    ),
    contract!(
        Details,
        "details",
        Container,
        AccessibilityRole::Region,
        true
    ),
    contract!(List, "list", Collection, AccessibilityRole::List, true),
    contract!(
        ListItem,
        "list-item",
        Collection,
        AccessibilityRole::ListItem,
        true
    ),
    contract!(Table, "table", Collection, AccessibilityRole::Region, true),
    contract!(Cell, "cell", Collection, AccessibilityRole::Region, false),
    contract!(Tree, "tree", Collection, AccessibilityRole::Region, true),
    contract!(
        EmptyState,
        "empty-state",
        Collection,
        AccessibilityRole::Status,
        false
    ),
    contract!(Slot, "slot", Shell, AccessibilityRole::Region, false),
    contract!(Surface, "surface", Shell, AccessibilityRole::Region, false),
    contract!(Widget, "widget", Shell, AccessibilityRole::Region, false),
];

pub(super) const fn str_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Slot of `tag` in [`ELEMENT_CONTRACT_DEFS`], evaluated at compile time.
///
/// Panicking here is deliberate: every caller passes a literal from
/// `contract_slots!`, so a tag that no longer exists in the definition list
/// fails the build instead of silently resolving to the wrong contract.
pub(super) const fn contract_slot_of(tag: &str) -> usize {
    let mut index = 0;
    while index < ELEMENT_CONTRACT_DEFS.len() {
        if str_eq(ELEMENT_CONTRACT_DEFS[index].tag, tag) {
            return index;
        }
        index += 1;
    }
    panic!("contract_slots! lists a tag that is not in ELEMENT_CONTRACT_DEFS");
}

/// Build the tag → slot dispatch used by [`element_contract_for_tag`].
///
/// A `match` over string literals lowers to a length switch plus direct
/// comparisons, so a lookup costs one dispatch instead of scanning all 62
/// definitions. Slots are resolved by `contract_slot_of` in an inline `const`
/// block, which keeps this list honest against `ELEMENT_CONTRACT_DEFS` at
/// compile time; `element_contract_dispatch_covers_every_definition` covers
/// the other direction (a definition with no arm here).
macro_rules! contract_slots {
    ($($tag:literal),* $(,)?) => {
        pub(super) fn contract_slot_for_tag(tag: &str) -> Option<usize> {
            match tag {
                $($tag => Some(const { contract_slot_of($tag) }),)*
                _ => None,
            }
        }
    };
}

contract_slots!(
    "box",
    "row",
    "column",
    "grid",
    "stack",
    "spacer",
    "divider",
    "separator",
    "scroll-area",
    "section",
    "header",
    "footer",
    "group",
    "form-row",
    "text",
    "icon",
    "image",
    "badge",
    "progress",
    "meter",
    "tooltip",
    "avatar",
    "shortcut",
    "button",
    "icon-button",
    "toggle-button",
    "command-button",
    "link-button",
    "input",
    "textarea",
    "search",
    "password",
    "number-input",
    "stepper",
    "select",
    "option",
    "checkbox",
    "switch",
    "radio",
    "radio-group",
    "segmented-control",
    "menu",
    "menu-item",
    "command-item",
    "preference-row",
    "panel",
    "popover",
    "dialog",
    "sheet",
    "tabs",
    "tab",
    "accordion",
    "details",
    "list",
    "list-item",
    "table",
    "cell",
    "tree",
    "empty-state",
    "slot",
    "surface",
    "widget",
);

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

pub(super) const fn field(
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

pub(super) const fn attr(
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

pub(super) const fn event(
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

pub(super) const fn element_type(
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
