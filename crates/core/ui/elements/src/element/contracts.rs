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
/// Panicking here is deliberate: every caller passes a literal from the
/// canonical schema, so a tag that no longer exists in the definition list
/// fails the build instead of silently resolving to the wrong contract.
pub(super) const fn contract_slot_of(tag: &str) -> usize {
    let mut index = 0;
    while index < ELEMENT_CONTRACT_DEFS.len() {
        if str_eq(ELEMENT_CONTRACT_DEFS[index].source_tag, tag) {
            return index;
        }
        index += 1;
    }
    panic!("element schema lists a tag that is not in ELEMENT_CONTRACT_DEFS");
}

static NO_EVENTS: &[ElementEventDef] = &[];
static ACTION_EVENTS: &[ElementEventDef] = &[
    event("click", "element", "Activation from pointer or keyboard"),
    event("change", "value", "Committed value change"),
    event("activate", "element", "Command or item activation"),
];
static INPUT_EVENTS: &[ElementEventDef] = &[
    event("input", "value", "Immediate value input"),
    event("change", "value", "Committed value change"),
];
static CHOICE_EVENTS: &[ElementEventDef] = &[
    event("change", "value", "Committed value change"),
    event("select", "value", "Selection change"),
    event("activate", "element", "Command or item activation"),
];
static ACTIVATE_EVENTS: &[ElementEventDef] =
    &[event("activate", "element", "Command or item activation")];
static OPEN_EVENTS: &[ElementEventDef] = &[event("openchange", "open", "Open state change")];
static SCROLL_EVENTS: &[ElementEventDef] = &[event("scroll", "offset", "Scroll position change")];

static LAYOUT_STYLE_HOOKS: &[&str] = &["layout"];
static STRUCTURE_STYLE_HOOKS: &[&str] = &["layout", "structure"];
static DISPLAY_STYLE_HOOKS: &[&str] = &["display"];
static PROGRESS_STYLE_HOOKS: &[&str] = &["display", "progress"];
static CONTROL_STYLE_HOOKS: &[&str] = &[
    "display",
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
];
static SHELL_STYLE_HOOKS: &[&str] = &["layout", "display", "structure", "expanded", "active"];

static EMPTY_FIELDS: &[ElementFieldDef] = &[];
static NO_DEFAULT_ATTRIBUTES: &[(&str, &str)] = &[];
static TEXTAREA_DEFAULT_ATTRIBUTES: &[(&str, &str)] = &[("multiline", "true")];
static PASSWORD_DEFAULT_ATTRIBUTES: &[(&str, &str)] = &[("masked", "true")];
static STEPPER_DEFAULT_ATTRIBUTES: &[(&str, &str)] = &[("step", "1")];

macro_rules! attribute_profile {
    (@core $(, $extra:expr)*) => {
        &[
            attr("id", ElementAttributeType::String, "Template id attribute"),
            attr("class", ElementAttributeType::String, "Style class list"),
            attr("style", ElementAttributeType::String, "Inline style rules"),
            attr("ref", ElementAttributeType::String, "Template ref attribute"),
            attr("data-mesh-element", ElementAttributeType::String, "Original source element tag before runtime lowering"),
            attr("label", ElementAttributeType::String, "Accessible label"),
            attr("aria-label", ElementAttributeType::String, "Accessible label"),
            attr("role", ElementAttributeType::String, "Accessibility role"),
            attr("aria-role", ElementAttributeType::String, "Accessibility role override"),
            attr("title", ElementAttributeType::String, "Accessible title"),
            $($extra),*
        ]
    };
}

macro_rules! schema_attributes {
    (Layout) => {
        attribute_profile!(@core,
            attr("align", ElementAttributeType::String, "Layout alignment"),
            attr("justify", ElementAttributeType::String, "Main-axis layout justification"),
            attr("spacing", ElementAttributeType::Number, "Layout spacing"),
            attr("gap", ElementAttributeType::Number, "Layout gap"),
            attr("width", ElementAttributeType::String, "Requested width"),
            attr("height", ElementAttributeType::String, "Requested height"),
            attr("min-width", ElementAttributeType::String, "Minimum width"),
            attr("max-width", ElementAttributeType::String, "Maximum width"),
            attr("min-height", ElementAttributeType::String, "Minimum height"),
            attr("max-height", ElementAttributeType::String, "Maximum height"),
            attr("overflow", ElementAttributeType::String, "Overflow behavior"),
            attr("overflow-x", ElementAttributeType::String, "Horizontal overflow behavior"),
            attr("overflow-y", ElementAttributeType::String, "Vertical overflow behavior"),
            attr("scroll-x", ElementAttributeType::Number, "Initial horizontal scroll offset"),
            attr("scroll-y", ElementAttributeType::Number, "Initial vertical scroll offset"),
            attr("columns", ElementAttributeType::String, "Conservative grid column track list"),
            attr("rows", ElementAttributeType::String, "Conservative grid row track list"),
            attr("column", ElementAttributeType::Number, "Grid column placement"),
            attr("row", ElementAttributeType::Number, "Grid row placement"),
            attr("column-span", ElementAttributeType::Number, "Grid column span"),
            attr("row-span", ElementAttributeType::Number, "Grid row span"),
            attr("layer", ElementAttributeType::Number, "Stacking layer"),
            attr("for", ElementAttributeType::String, "Associated element id")
        )
    };
    (Display) => {
        attribute_profile!(@core,
            attr("value", ElementAttributeType::String, "Current value"),
            attr("min", ElementAttributeType::Number, "Minimum value"),
            attr("max", ElementAttributeType::Number, "Maximum value"),
            attr("src", ElementAttributeType::String, "Image or icon source"),
            attr("name", ElementAttributeType::String, "Icon or shortcut name"),
            attr("alt", ElementAttributeType::String, "Accessible alternate text"),
            attr("size", ElementAttributeType::Number, "Display size hint"),
            attr("key", ElementAttributeType::String, "Shortcut key label"),
            attr("tooltip", ElementAttributeType::String, "Tooltip text"),
            attr("tooltip-for", ElementAttributeType::String, "Tooltip owner element id"),
            attr("indeterminate", ElementAttributeType::Boolean, "Progress has no determinate value")
        )
    };
    (Action) => {
        attribute_profile!(@core,
            attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
            attr("busy", ElementAttributeType::Boolean, "Busy state"),
            attr("default", ElementAttributeType::Boolean, "Default action state"),
            attr("destructive", ElementAttributeType::Boolean, "Destructive action state"),
            attr("pressed", ElementAttributeType::Boolean, "Pressed state"),
            attr("invalid", ElementAttributeType::Boolean, "Invalid state"),
            attr("keybind", ElementAttributeType::String, "Associated keybind id or display shortcut"),
            attr("command", ElementAttributeType::String, "Command intent metadata"),
            attr("href", ElementAttributeType::String, "Link intent metadata"),
            attr("value", ElementAttributeType::String, "Current value")
        )
    };
    (TextInput) => {
        attribute_profile!(@core,
            attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
            attr("readonly", ElementAttributeType::Boolean, "Read-only state"),
            attr("required", ElementAttributeType::Boolean, "Required state"),
            attr("invalid", ElementAttributeType::Boolean, "Invalid state"),
            attr("value", ElementAttributeType::String, "Current value"),
            attr("min", ElementAttributeType::Number, "Minimum value"),
            attr("max", ElementAttributeType::Number, "Maximum value"),
            attr("type", ElementAttributeType::String, "Input type metadata"),
            attr("placeholder", ElementAttributeType::String, "Input placeholder text"),
            attr("multiline", ElementAttributeType::Boolean, "Input accepts multiple lines"),
            attr("masked", ElementAttributeType::Boolean, "Input masks displayed text"),
            attr("step", ElementAttributeType::Number, "Numeric step size")
        )
    };
    (ChoiceMenu) => {
        attribute_profile!(@core,
            attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
            attr("required", ElementAttributeType::Boolean, "Required state"),
            attr("invalid", ElementAttributeType::Boolean, "Invalid state"),
            attr("checked", ElementAttributeType::Boolean, "Checked state"),
            attr("selected", ElementAttributeType::Boolean, "Selected state"),
            attr("expanded", ElementAttributeType::Boolean, "Expanded state"),
            attr("value", ElementAttributeType::String, "Current value")
        )
    };
    (Container) => {
        attribute_profile!(@core,
            attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
            attr("hidden", ElementAttributeType::Boolean, "Hidden state"),
            attr("selected", ElementAttributeType::Boolean, "Selected state"),
            attr("expanded", ElementAttributeType::Boolean, "Expanded state"),
            attr("open", ElementAttributeType::Boolean, "Open state"),
            attr("active", ElementAttributeType::Boolean, "Active state")
        )
    };
    (Collection) => {
        attribute_profile!(@core,
            attr("disabled", ElementAttributeType::Boolean, "Disabled state"),
            attr("hidden", ElementAttributeType::Boolean, "Hidden state"),
            attr("selected", ElementAttributeType::Boolean, "Selected state"),
            attr("expanded", ElementAttributeType::Boolean, "Expanded state"),
            attr("open", ElementAttributeType::Boolean, "Open state"),
            attr("active", ElementAttributeType::Boolean, "Active state")
        )
    };
    (Shell) => {
        attribute_profile!(@core,
            attr("hidden", ElementAttributeType::Boolean, "Hidden state"),
            attr("expanded", ElementAttributeType::Boolean, "Expanded state"),
            attr("open", ElementAttributeType::Boolean, "Open state")
        )
    };
}

/// The canonical element schema. The registry/type/lowering/accessibility
/// records below are all generated from this one list.
macro_rules! element_schema {
    ($callback:ident) => {
        $callback! {
            (Box, "box", "box", "MeshElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Row, "row", "row", "RowElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Column, "column", "column", "ColumnElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Grid, "grid", "box", "GridElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Stack, "stack", "box", "StackElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Scroll, "scroll", "scroll", "ScrollElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, SCROLL_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (ScrollView, "scroll-view", "scroll", "ScrollElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, SCROLL_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (ScrollArea, "scroll-area", "scroll", "ScrollElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, SCROLL_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Spacer, "spacer", "box", "SpacerElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Divider, "divider", "box", "SeparatorElement", Layout, AccessibilityRole::Separator, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Separator, "separator", "box", "SeparatorElement", Layout, AccessibilityRole::Separator, false, false, EMPTY_FIELDS, NO_EVENTS, LAYOUT_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Section, "section", "box", "SectionElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Header, "header", "box", "HeaderElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Footer, "footer", "box", "FooterElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Group, "group", "box", "GroupElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (FormRow, "form-row", "box", "FormRowElement", Layout, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Text, "text", "text", "TextElement", Display, AccessibilityRole::Label, false, false, TEXT_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Label, "label", "text", "LabelElement", Display, AccessibilityRole::Label, false, false, LABEL_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Icon, "icon", "icon", "IconElement", Display, AccessibilityRole::Image, false, false, ICON_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Image, "image", "image", "ImageElement", Display, AccessibilityRole::Image, false, false, IMAGE_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Badge, "badge", "text", "TextElement", Display, AccessibilityRole::Status, false, false, TEXT_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Progress, "progress", "text", "ProgressElement", Display, AccessibilityRole::ProgressBar, false, false, EMPTY_FIELDS, NO_EVENTS, PROGRESS_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Meter, "meter", "text", "MeterElement", Display, AccessibilityRole::ProgressBar, false, false, EMPTY_FIELDS, NO_EVENTS, PROGRESS_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Tooltip, "tooltip", "text", "TooltipElement", Display, AccessibilityRole::Alert, false, false, EMPTY_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Avatar, "avatar", "icon", "AvatarElement", Display, AccessibilityRole::Image, false, false, ICON_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Shortcut, "shortcut", "text", "TextElement", Display, AccessibilityRole::Label, false, false, TEXT_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Button, "button", "button", "ButtonElement", Action, AccessibilityRole::Button, true, true, BUTTON_FIELDS, ACTION_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (IconButton, "icon-button", "button", "IconButtonElement", Action, AccessibilityRole::Button, true, true, BUTTON_FIELDS, ACTION_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (ToggleButton, "toggle-button", "button", "ButtonElement", Action, AccessibilityRole::Button, true, true, BUTTON_FIELDS, ACTION_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (CommandButton, "command-button", "button", "ButtonElement", Action, AccessibilityRole::Button, true, true, BUTTON_FIELDS, ACTION_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (LinkButton, "link-button", "button", "ButtonElement", Action, AccessibilityRole::Button, true, true, BUTTON_FIELDS, ACTION_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Input, "input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (TextArea, "textarea", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("textarea"), TEXTAREA_DEFAULT_ATTRIBUTES),
            (Search, "search", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("search"), NO_DEFAULT_ATTRIBUTES),
            (Password, "password", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("password"), PASSWORD_DEFAULT_ATTRIBUTES),
            (NumberInput, "number-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("number"), NO_DEFAULT_ATTRIBUTES),
            (Stepper, "stepper", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("number"), STEPPER_DEFAULT_ATTRIBUTES),
            (TextInput, "text-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("text"), NO_DEFAULT_ATTRIBUTES),
            (PasswordInput, "password-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("password"), PASSWORD_DEFAULT_ATTRIBUTES),
            (SearchInput, "search-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("search"), NO_DEFAULT_ATTRIBUTES),
            (EmailInput, "email-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("email"), NO_DEFAULT_ATTRIBUTES),
            (UrlInput, "url-input", "input", "InputElement", TextInput, AccessibilityRole::TextInput, true, true, INPUT_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, Some("url"), NO_DEFAULT_ATTRIBUTES),
            (Slider, "slider", "slider", "SliderElement", TextInput, AccessibilityRole::Slider, true, true, SLIDER_FIELDS, INPUT_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Select, "select", "input", "SelectElement", ChoiceMenu, AccessibilityRole::Menu, true, true, EMPTY_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Option, "option", "input", "OptionElement", ChoiceMenu, AccessibilityRole::MenuItem, false, false, EMPTY_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Switch, "switch", "input", "SwitchElement", ChoiceMenu, AccessibilityRole::Switch, true, true, CHECKABLE_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Checkbox, "checkbox", "input", "CheckboxElement", ChoiceMenu, AccessibilityRole::Checkbox, true, true, CHECKABLE_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Radio, "radio", "input", "CheckboxElement", ChoiceMenu, AccessibilityRole::Checkbox, true, true, CHECKABLE_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (RadioGroup, "radio-group", "input", "RadioGroupElement", ChoiceMenu, AccessibilityRole::Region, false, false, EMPTY_FIELDS, CHOICE_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (SegmentedControl, "segmented-control", "input", "SegmentedControlElement", ChoiceMenu, AccessibilityRole::Toolbar, true, true, EMPTY_FIELDS, CHOICE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Menu, "menu", "row", "MenuElement", ChoiceMenu, AccessibilityRole::Menu, true, true, EMPTY_FIELDS, ACTIVATE_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (MenuItem, "menu-item", "row", "MenuItemElement", ChoiceMenu, AccessibilityRole::MenuItem, true, true, EMPTY_FIELDS, ACTIVATE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (CommandItem, "command-item", "row", "MenuItemElement", ChoiceMenu, AccessibilityRole::MenuItem, true, true, EMPTY_FIELDS, ACTIVATE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (PreferenceRow, "preference-row", "row", "PreferenceRowElement", ChoiceMenu, AccessibilityRole::Region, true, false, EMPTY_FIELDS, ACTIVATE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Panel, "panel", "box", "PanelElement", Container, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Popover, "popover", "box", "PopoverElement", Container, AccessibilityRole::Dialog, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Dialog, "dialog", "box", "DialogElement", Container, AccessibilityRole::Dialog, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Sheet, "sheet", "box", "SheetElement", Container, AccessibilityRole::Dialog, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Tabs, "tabs", "box", "TabsElement", Container, AccessibilityRole::Toolbar, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Tab, "tab", "box", "TabElement", Container, AccessibilityRole::Tab, true, true, EMPTY_FIELDS, ACTIVATE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Accordion, "accordion", "box", "AccordionElement", Container, AccessibilityRole::Region, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Details, "details", "box", "DetailsElement", Container, AccessibilityRole::Region, true, true, EMPTY_FIELDS, OPEN_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (List, "list", "column", "ListElement", Collection, AccessibilityRole::List, true, true, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (ListItem, "list-item", "row", "ListItemElement", Collection, AccessibilityRole::ListItem, true, true, EMPTY_FIELDS, ACTIVATE_EVENTS, CONTROL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Table, "table", "column", "TableElement", Collection, AccessibilityRole::Region, true, true, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Cell, "cell", "row", "CellElement", Collection, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Tree, "tree", "column", "TreeElement", Collection, AccessibilityRole::Region, true, true, EMPTY_FIELDS, NO_EVENTS, STRUCTURE_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (EmptyState, "empty-state", "row", "EmptyStateElement", Collection, AccessibilityRole::Status, false, false, EMPTY_FIELDS, NO_EVENTS, DISPLAY_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Slot, "slot", "box", "SlotElement", Shell, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Surface, "surface", "box", "SurfaceElement", Shell, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
            (Widget, "widget", "box", "WidgetElement", Shell, AccessibilityRole::Region, false, false, EMPTY_FIELDS, NO_EVENTS, SHELL_STYLE_HOOKS, None, NO_DEFAULT_ATTRIBUTES),
        }
    };
}

macro_rules! generate_element_registries {
    ($(($kind:ident, $tag:literal, $runtime_tag:literal, $type_name:literal, $family:ident, $role:expr, $focusable:expr, $label_required:expr, $fields:ident, $events:ident, $style_hooks:ident, $input_type:expr, $default_attributes:ident)),* $(,)?) => {
        pub static ELEMENT_CONTRACT_DEFS: &[ElementContractDef] = &[
            $(ElementContractDef {
                kind: ElementKind::$kind,
                source_tag: $tag,
                tag: $tag,
                runtime_tag: $runtime_tag,
                family: ElementFamily::$family,
                type_name: $type_name,
                attributes: schema_attributes!($family),
                states: COMMON_STATES,
                events: $events,
                accessibility: ElementAccessibilityDef {
                    role: $role,
                    focusable: $focusable,
                    label_required: $label_required,
                },
                style_hooks: $style_hooks,
                input_type: $input_type,
                default_attributes: $default_attributes,
            }),*
        ];

        pub static ELEMENT_TYPE_DEFS: &[ElementTypeDef] = &[
            $(element_type_with_runtime(
                ElementKind::$kind,
                $tag,
                $runtime_tag,
                $type_name,
                $fields,
            )),*
        ];

        pub(super) fn contract_slot_for_tag(tag: &str) -> Option<usize> {
            match tag {
                $($tag => Some(const { contract_slot_of($tag) }),)*
                _ => None,
            }
        }
    };
}

element_schema!(generate_element_registries);

pub(super) static UNKNOWN_ELEMENT_TYPE: ElementTypeDef = ElementTypeDef {
    kind: ElementKind::Unknown,
    source_tag: "unknown",
    tag: "unknown",
    runtime_tag: "unknown",
    type_name: "MeshElement",
    fields: EMPTY_FIELDS,
};

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

pub(super) const fn element_type_with_runtime(
    kind: ElementKind,
    tag: &'static str,
    runtime_tag: &'static str,
    type_name: &'static str,
    fields: &'static [ElementFieldDef],
) -> ElementTypeDef {
    ElementTypeDef {
        kind,
        source_tag: tag,
        tag,
        runtime_tag,
        type_name,
        fields,
    }
}
